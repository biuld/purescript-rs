use crate::{RawToken, RawTokenKind};
use psrs_span::{SourceFile, TextRange};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutToken {
    pub kind: LayoutTokenKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutTokenKind {
    Raw(RawTokenKind),
    LayoutStart,
    LayoutSep,
    LayoutEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockKind {
    Where,
    Let,
    Of,
    Do,
    Ado,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameKind {
    Block(BlockKind),
    Paren,
    Bracket,
    Brace,
    Case,
    If,
    Then,
    Guard,
}

impl FrameKind {
    fn is_block(self) -> bool {
        matches!(self, FrameKind::Block(_))
    }
}

#[derive(Clone, Copy, Debug)]
struct Frame {
    indent: usize,
    kind: FrameKind,
}

/// Inserts the first-stage PureScript offside markers into the raw token stream.
/// The implementation covers implicit `where`, `let`, `of`, `do`, and `ado`
/// blocks and the ways each one is closed: an outdent, a closing delimiter, a
/// comma, `where`, or `in`.
pub fn add_layout(source: &SourceFile, tokens: &[RawToken]) -> Vec<LayoutToken> {
    let mut result = Vec::with_capacity(tokens.len() + 8);
    let mut frames: Vec<Frame> = Vec::new();
    let mut pending: Option<BlockKind> = None;
    let mut crossed_newline = false;

    for token in tokens {
        if token.kind == RawTokenKind::Newline {
            crossed_newline = true;
            continue;
        }

        if matches!(token.kind, RawTokenKind::Eof) {
            while let Some(frame) = frames.pop() {
                if frame.kind.is_block() {
                    result.push(virtual_token(LayoutTokenKind::LayoutEnd, token.span.start));
                }
            }
            result.push(raw(token));
            break;
        }

        let (_, column) = source.line_column(token.span.start);
        let indent = column - 1;
        let continues = matches!(
            token.kind,
            RawTokenKind::Operator(_)
                | RawTokenKind::Backtick
                | RawTokenKind::DotDot
                | RawTokenKind::Then
                | RawTokenKind::Else
                | RawTokenKind::Pipe
        );

        match token.kind {
            RawTokenKind::Comma => {
                close_indented(&mut frames, &mut result, token.span.start, |kind| {
                    matches!(kind, BlockKind::Do | BlockKind::Ado | BlockKind::Let)
                });
            }
            RawTokenKind::RParen | RawTokenKind::RBracket | RawTokenKind::RBrace => {
                close_indented(&mut frames, &mut result, token.span.start, |_| true);
                pop_paired_delimiter(&mut frames, &token.kind);
            }
            RawTokenKind::Where => {
                close_for_where(&mut frames, &mut result, token.span.start, indent);
            }
            RawTokenKind::In => {
                close_through_let(&mut frames, &mut result, token.span.start);
            }
            RawTokenKind::Of => {
                close_indented(&mut frames, &mut result, token.span.start, |_| true);
                if let Some(index) = frames
                    .iter()
                    .rposition(|frame| frame.kind == FrameKind::Case)
                {
                    frames.truncate(index);
                }
            }
            RawTokenKind::Else => {
                let boundary = frames
                    .iter()
                    .rposition(|frame| frame.kind == FrameKind::Then);
                if let Some(index) = boundary {
                    while frames.len() > index {
                        let frame = frames.pop().expect("index is within bounds");
                        if frame.kind.is_block() {
                            result
                                .push(virtual_token(LayoutTokenKind::LayoutEnd, token.span.start));
                        }
                    }
                } else if crossed_newline {
                    while frames
                        .last()
                        .is_some_and(|frame| frame.kind.is_block() && indent < frame.indent)
                    {
                        frames.pop();
                        result.push(virtual_token(LayoutTokenKind::LayoutEnd, token.span.start));
                    }
                }
            }
            RawTokenKind::Arrow | RawTokenKind::Equals => {
                if let Some(index) = frames
                    .iter()
                    .rposition(|frame| frame.kind == FrameKind::Guard)
                {
                    frames.truncate(index);
                }
            }
            _ => {
                if crossed_newline {
                    while frames
                        .last()
                        .is_some_and(|frame| frame.kind.is_block() && indent < frame.indent)
                    {
                        frames.pop();
                        result.push(virtual_token(LayoutTokenKind::LayoutEnd, token.span.start));
                    }
                    if !continues
                        && frames
                            .last()
                            .is_some_and(|frame| frame.kind.is_block() && indent == frame.indent)
                    {
                        result.push(virtual_token(LayoutTokenKind::LayoutSep, token.span.start));
                    }
                }
            }
        }
        crossed_newline = false;

        if let Some(kind) = pending.take() {
            let enclosing = frames
                .iter()
                .rev()
                .find(|frame| frame.kind.is_block())
                .map(|frame| frame.indent as isize)
                .unwrap_or(-1);
            if indent as isize > enclosing {
                result.push(virtual_token(
                    LayoutTokenKind::LayoutStart,
                    token.span.start,
                ));
                frames.push(Frame {
                    indent,
                    kind: FrameKind::Block(kind),
                });
            }
        }

        match token.kind {
            RawTokenKind::LParen => frames.push(Frame {
                indent,
                kind: FrameKind::Paren,
            }),
            RawTokenKind::LBracket => frames.push(Frame {
                indent,
                kind: FrameKind::Bracket,
            }),
            RawTokenKind::LBrace => frames.push(Frame {
                indent,
                kind: FrameKind::Brace,
            }),
            RawTokenKind::Case => frames.push(Frame {
                indent,
                kind: FrameKind::Case,
            }),
            RawTokenKind::If => frames.push(Frame {
                indent,
                kind: FrameKind::If,
            }),
            RawTokenKind::Then => {
                if let Some(index) = frames.iter().rposition(|frame| frame.kind == FrameKind::If) {
                    frames.truncate(index);
                }
                frames.push(Frame {
                    indent,
                    kind: FrameKind::Then,
                });
            }
            RawTokenKind::Pipe => {
                let enclosing = frames.iter().rev().find_map(|frame| match frame.kind {
                    FrameKind::Block(kind) => Some(kind),
                    _ => None,
                });
                if matches!(enclosing, Some(BlockKind::Of | BlockKind::Let)) {
                    frames.push(Frame {
                        indent,
                        kind: FrameKind::Guard,
                    });
                }
            }
            _ => {}
        }

        result.push(raw(token));

        pending = match token.kind {
            RawTokenKind::Where => Some(BlockKind::Where),
            RawTokenKind::Let => Some(BlockKind::Let),
            RawTokenKind::Of => Some(BlockKind::Of),
            RawTokenKind::Do => Some(BlockKind::Do),
            RawTokenKind::Ado => Some(BlockKind::Ado),
            _ => None,
        };
    }
    result
}

fn close_indented(
    frames: &mut Vec<Frame>,
    result: &mut Vec<LayoutToken>,
    at: u32,
    close: impl Fn(BlockKind) -> bool,
) {
    while let Some(frame) = frames.last().copied() {
        let FrameKind::Block(kind) = frame.kind else {
            break;
        };
        if close(kind) {
            frames.pop();
            result.push(virtual_token(LayoutTokenKind::LayoutEnd, at));
        } else {
            break;
        }
    }
}

fn pop_paired_delimiter(frames: &mut Vec<Frame>, token: &RawTokenKind) {
    let wanted = match token {
        RawTokenKind::RParen => FrameKind::Paren,
        RawTokenKind::RBracket => FrameKind::Bracket,
        RawTokenKind::RBrace => FrameKind::Brace,
        _ => return,
    };
    if let Some(index) = frames.iter().rposition(|frame| frame.kind == wanted) {
        frames.truncate(index);
    }
}

fn close_for_where(frames: &mut Vec<Frame>, result: &mut Vec<LayoutToken>, at: u32, indent: usize) {
    while let Some(frame) = frames.last().copied() {
        let FrameKind::Block(kind) = frame.kind else {
            break;
        };
        if kind == BlockKind::Do || indent <= frame.indent {
            frames.pop();
            result.push(virtual_token(LayoutTokenKind::LayoutEnd, at));
        } else {
            break;
        }
    }
}

fn close_through_let(frames: &mut Vec<Frame>, result: &mut Vec<LayoutToken>, at: u32) {
    if let Some(index) = frames
        .iter()
        .rposition(|frame| frame.kind == FrameKind::Block(BlockKind::Let))
    {
        while frames.len() > index {
            frames.pop();
            result.push(virtual_token(LayoutTokenKind::LayoutEnd, at));
        }
    }
}

fn raw(token: &RawToken) -> LayoutToken {
    LayoutToken {
        kind: LayoutTokenKind::Raw(token.kind.clone()),
        span: token.span,
    }
}

fn virtual_token(kind: LayoutTokenKind, at: u32) -> LayoutToken {
    LayoutToken {
        kind,
        span: TextRange::empty(at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex;

    fn labels(source: &str) -> Vec<&'static str> {
        let source_file = SourceFile::new("test.purs", source);
        let (tokens, errors) = lex(source);
        assert!(errors.is_empty(), "{errors:?}");
        add_layout(&source_file, &tokens)
            .into_iter()
            .map(|token| match token.kind {
                LayoutTokenKind::LayoutStart => "<start>",
                LayoutTokenKind::LayoutSep => "<sep>",
                LayoutTokenKind::LayoutEnd => "<end>",
                LayoutTokenKind::Raw(RawTokenKind::LowerIdent(_)) => "ident",
                LayoutTokenKind::Raw(RawTokenKind::UpperIdent(_)) => "upper",
                LayoutTokenKind::Raw(RawTokenKind::Integer(_)) => "int",
                LayoutTokenKind::Raw(RawTokenKind::Let) => "let",
                LayoutTokenKind::Raw(RawTokenKind::In) => "in",
                LayoutTokenKind::Raw(RawTokenKind::Where) => "where",
                LayoutTokenKind::Raw(RawTokenKind::Equals) => "=",
                LayoutTokenKind::Raw(RawTokenKind::Eof) => "eof",
                _ => "other",
            })
            .collect()
    }

    #[test]
    fn inserts_module_and_let_layout() {
        assert_eq!(
            labels("module Main where\nmain =\n  let\n    x = 40\n    y = 2\n  in x\n"),
            [
                "other", "upper", "where", "<start>", "ident", "=", "let", "<start>", "ident", "=",
                "int", "<sep>", "ident", "=", "int", "<end>", "in", "ident", "<end>", "eof"
            ]
        );
    }

    #[test]
    fn closes_blocks_at_closing_delimiters_and_commas() {
        assert_eq!(
            labels("main = f (do\n  a\n  b)\n"),
            [
                "ident", "=", "ident", "other", "other", "<start>", "ident", "<sep>", "ident",
                "<end>", "other", "eof"
            ]
        );
    }
}

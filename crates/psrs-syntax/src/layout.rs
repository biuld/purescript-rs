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

#[derive(Clone, Copy, Debug)]
struct Block {
    indent: usize,
    kind: BlockKind,
}

/// Inserts the first-stage PureScript offside markers into the raw token stream.
/// The implementation covers implicit `where`, `let`, `of`, `do`, and `ado` blocks.
pub fn add_layout(source: &SourceFile, tokens: &[RawToken]) -> Vec<LayoutToken> {
    let mut result = Vec::with_capacity(tokens.len() + 8);
    let mut blocks: Vec<Block> = Vec::new();
    let mut pending: Option<BlockKind> = None;
    let mut crossed_newline = false;

    for token in tokens {
        if token.kind == RawTokenKind::Newline {
            crossed_newline = true;
            continue;
        }

        if matches!(token.kind, RawTokenKind::Eof) {
            while blocks.pop().is_some() {
                result.push(virtual_token(LayoutTokenKind::LayoutEnd, token.span.start));
            }
            result.push(raw(token));
            break;
        }

        let (_, column) = source.line_column(token.span.start);
        let indent = column - 1;

        if matches!(token.kind, RawTokenKind::In) {
            close_through_let(&mut blocks, &mut result, token.span.start);
        } else if crossed_newline {
            while blocks.last().is_some_and(|block| indent < block.indent) {
                blocks.pop();
                result.push(virtual_token(LayoutTokenKind::LayoutEnd, token.span.start));
            }
            // A line that starts with an operator continues the previous
            // expression instead of starting a new layout item.
            let continues_expression = matches!(
                token.kind,
                RawTokenKind::Operator(_) | RawTokenKind::Backtick
            );
            if !continues_expression && blocks.last().is_some_and(|block| indent == block.indent) {
                result.push(virtual_token(LayoutTokenKind::LayoutSep, token.span.start));
            }
        }
        crossed_newline = false;

        if let Some(kind) = pending.take()
            && !matches!(token.kind, RawTokenKind::LBrace)
        {
            let enclosing = blocks
                .last()
                .map(|block| block.indent as isize)
                .unwrap_or(-1);
            if indent as isize > enclosing {
                result.push(virtual_token(
                    LayoutTokenKind::LayoutStart,
                    token.span.start,
                ));
                blocks.push(Block { indent, kind });
            }
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

fn close_through_let(blocks: &mut Vec<Block>, result: &mut Vec<LayoutToken>, at: u32) {
    if let Some(index) = blocks
        .iter()
        .rposition(|block| block.kind == BlockKind::Let)
    {
        while blocks.len() > index {
            blocks.pop();
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
}

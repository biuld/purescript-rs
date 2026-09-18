use chumsky::{prelude::*, text};
use psrs_span::TextRange;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawToken {
    pub kind: RawTokenKind,
    pub span: TextRange,
}

impl RawToken {
    fn new(kind: RawTokenKind, span: Range<usize>) -> Self {
        Self {
            kind,
            span: TextRange::new(span.start as u32, span.end as u32),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RawTokenKind {
    LowerIdent(String),
    UpperIdent(String),
    Integer(String),
    String(String),
    Char(char),
    Operator(String),

    Module,
    Where,
    Import,
    As,
    Hiding,
    Data,
    Newtype,
    Type,
    Class,
    Instance,
    Derive,
    Let,
    In,
    If,
    Then,
    Else,
    Case,
    Of,
    Do,
    Ado,
    Forall,

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Colon,
    DoubleColon,
    Equals,
    Arrow,
    LeftArrow,
    Pipe,
    Backslash,
    Newline,
    Eof,
}

impl RawTokenKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::LowerIdent(_) => "LowerIdent",
            Self::UpperIdent(_) => "UpperIdent",
            Self::Integer(_) => "Integer",
            Self::String(_) => "String",
            Self::Char(_) => "Char",
            Self::Operator(_) => "Operator",
            Self::Module => "Module",
            Self::Where => "Where",
            Self::Import => "Import",
            Self::As => "As",
            Self::Hiding => "Hiding",
            Self::Data => "Data",
            Self::Newtype => "Newtype",
            Self::Type => "Type",
            Self::Class => "Class",
            Self::Instance => "Instance",
            Self::Derive => "Derive",
            Self::Let => "Let",
            Self::In => "In",
            Self::If => "If",
            Self::Then => "Then",
            Self::Else => "Else",
            Self::Case => "Case",
            Self::Of => "Of",
            Self::Do => "Do",
            Self::Ado => "Ado",
            Self::Forall => "Forall",
            Self::LParen => "LParen",
            Self::RParen => "RParen",
            Self::LBrace => "LBrace",
            Self::RBrace => "RBrace",
            Self::LBracket => "LBracket",
            Self::RBracket => "RBracket",
            Self::Comma => "Comma",
            Self::Dot => "Dot",
            Self::Colon => "Colon",
            Self::DoubleColon => "DoubleColon",
            Self::Equals => "Equals",
            Self::Arrow => "Arrow",
            Self::LeftArrow => "LeftArrow",
            Self::Pipe => "Pipe",
            Self::Backslash => "Backslash",
            Self::Newline => "Newline",
            Self::Eof => "Eof",
        }
    }

    pub fn source_text(&self) -> Option<&str> {
        match self {
            Self::LowerIdent(text)
            | Self::UpperIdent(text)
            | Self::Integer(text)
            | Self::Operator(text) => Some(text),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    pub span: TextRange,
    pub message: String,
}

fn keyword(text: String) -> RawTokenKind {
    match text.as_str() {
        "module" => RawTokenKind::Module,
        "where" => RawTokenKind::Where,
        "import" => RawTokenKind::Import,
        "as" => RawTokenKind::As,
        "hiding" => RawTokenKind::Hiding,
        "data" => RawTokenKind::Data,
        "newtype" => RawTokenKind::Newtype,
        "type" => RawTokenKind::Type,
        "class" => RawTokenKind::Class,
        "instance" => RawTokenKind::Instance,
        "derive" => RawTokenKind::Derive,
        "let" => RawTokenKind::Let,
        "in" => RawTokenKind::In,
        "if" => RawTokenKind::If,
        "then" => RawTokenKind::Then,
        "else" => RawTokenKind::Else,
        "case" => RawTokenKind::Case,
        "of" => RawTokenKind::Of,
        "do" => RawTokenKind::Do,
        "ado" => RawTokenKind::Ado,
        "forall" => RawTokenKind::Forall,
        _ if text.starts_with(|c: char| c.is_ascii_uppercase()) => RawTokenKind::UpperIdent(text),
        _ => RawTokenKind::LowerIdent(text),
    }
}

fn classify_operator(text: String) -> RawTokenKind {
    match text.as_str() {
        "=" => RawTokenKind::Equals,
        "::" => RawTokenKind::DoubleColon,
        "->" => RawTokenKind::Arrow,
        "<-" => RawTokenKind::LeftArrow,
        "|" => RawTokenKind::Pipe,
        "." => RawTokenKind::Dot,
        ":" => RawTokenKind::Colon,
        _ => RawTokenKind::Operator(text),
    }
}

fn lexer() -> impl Parser<char, Vec<RawToken>, Error = Simple<char>> {
    let identifier = filter(|c: &char| c.is_ascii_alphabetic() || *c == '_')
        .then(filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_' || *c == '\'').repeated())
        .map(|(first, rest)| std::iter::once(first).chain(rest).collect::<String>())
        .map(keyword);

    let escape = just('\\').ignore_then(choice((
        just('n').to('\n'),
        just('r').to('\r'),
        just('t').to('\t'),
        just('0').to('\0'),
        just('"').to('"'),
        just('\'').to('\''),
        just('\\').to('\\'),
    )));

    let string = just('"')
        .ignore_then(
            choice((escape, none_of("\"\\\n")))
                .repeated()
                .collect::<String>(),
        )
        .then_ignore(just('"'))
        .map(RawTokenKind::String);

    let character = just('\'')
        .ignore_then(choice((escape, none_of("'\\\n"))))
        .then_ignore(just('\''))
        .map(RawTokenKind::Char);

    let operator = filter(|c: &char| "!#$%&*+./<=>?@\\^|-~:".contains(*c))
        .repeated()
        .at_least(1)
        .collect::<String>()
        .map(classify_operator);

    let token = choice((
        text::int(10).map(RawTokenKind::Integer),
        identifier,
        string,
        character,
        just('(').to(RawTokenKind::LParen),
        just(')').to(RawTokenKind::RParen),
        just('{').to(RawTokenKind::LBrace),
        just('}').to(RawTokenKind::RBrace),
        just('[').to(RawTokenKind::LBracket),
        just(']').to(RawTokenKind::RBracket),
        just(',').to(RawTokenKind::Comma),
        just('\\').to(RawTokenKind::Backslash),
        operator,
    ));

    let token = token.map_with_span(|kind, span| Some(RawToken::new(kind, span)));
    let newline = just('\n')
        .map(|_| RawTokenKind::Newline)
        .map_with_span(|kind, span| Some(RawToken::new(kind, span)));
    let comment = just("--")
        .ignore_then(none_of("\n").repeated())
        .ignored()
        .to(None);
    let whitespace = one_of(" \t\r").repeated().at_least(1).ignored().to(None);

    choice((comment, whitespace, newline, token))
        .repeated()
        .then_ignore(end())
        .map(|tokens| tokens.into_iter().flatten().collect())
}

pub fn lex(source: &str) -> (Vec<RawToken>, Vec<LexError>) {
    let (tokens, errors) = lexer().parse_recovery(source);
    let mut byte_offsets: Vec<u32> = source
        .char_indices()
        .map(|(offset, _)| offset as u32)
        .collect();
    byte_offsets.push(source.len() as u32);
    let mut tokens = tokens.unwrap_or_default();
    for token in &mut tokens {
        token.span = byte_range(
            &byte_offsets,
            token.span.start as usize,
            token.span.end as usize,
        );
    }
    let errors = errors
        .into_iter()
        .map(|error| {
            let span = error.span();
            LexError {
                span: byte_range(&byte_offsets, span.start, span.end),
                message: format!("unexpected character {:?}", error.found().copied()),
            }
        })
        .collect();
    tokens.push(RawToken {
        kind: RawTokenKind::Eof,
        span: TextRange::empty(source.len() as u32),
    });
    (tokens, errors)
}

fn byte_range(byte_offsets: &[u32], start: usize, end: usize) -> TextRange {
    let eof = *byte_offsets.last().unwrap_or(&0);
    TextRange::new(
        byte_offsets.get(start).copied().unwrap_or(eof),
        byte_offsets.get(end).copied().unwrap_or(eof),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_keywords_operators_comments_and_newlines() {
        let (tokens, errors) = lex("module Main where\n  answer = 40 + 2 -- ok\n");
        assert!(errors.is_empty(), "{errors:?}");
        let labels: Vec<_> = tokens.iter().map(|token| token.kind.label()).collect();
        assert_eq!(
            labels,
            [
                "Module",
                "UpperIdent",
                "Where",
                "Newline",
                "LowerIdent",
                "Equals",
                "Integer",
                "Operator",
                "Integer",
                "Newline",
                "Eof",
            ]
        );
        assert_eq!(tokens[7].kind, RawTokenKind::Operator("+".into()));
    }

    #[test]
    fn reports_invalid_characters_with_spans() {
        let (_, errors) = lex("main = §");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].span, TextRange::new(7, 9));
    }

    #[test]
    fn decodes_common_string_and_character_escapes() {
        let (tokens, errors) = lex(r#"main = "line\nnext""#);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == RawTokenKind::String("line\nnext".into()))
        );

        let (tokens, errors) = lex(r#"main = '\t'"#);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == RawTokenKind::Char('\t'))
        );
    }
}

use super::literals::{lex_char, lex_number, lex_string};
use super::scanner::{
    Scanner, classify_operator, is_identifier_char, is_identifier_start, is_symbol_char,
    is_uppercase_start, keyword,
};
use super::{LexError, RawToken, RawTokenKind};
use psrs_span::TextRange;

pub fn lex(source: &str) -> (Vec<RawToken>, Vec<LexError>) {
    let mut scanner = Scanner::new(source);
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut seen_token = false;

    while scanner.peek().is_some() {
        let start = scanner.position();
        let character = scanner.peek().unwrap();
        match character {
            '\n' => {
                scanner.bump();
                tokens.push(RawToken::new(RawTokenKind::Newline, start..start + 1));
            }
            ' ' | '\r' => {
                scanner.bump();
            }
            '#' if !seen_token && scanner.peek_at(1) == Some('!') => {
                while scanner.peek().is_some_and(|c| c != '\n') {
                    scanner.bump();
                }
            }
            '-' if scanner.peek_at(1) == Some('-') => {
                while scanner.peek().is_some_and(|c| c != '\n') {
                    scanner.bump();
                }
            }
            '{' if scanner.peek_at(1) == Some('-') => {
                scanner.bump();
                scanner.bump();
                while scanner.peek().is_some() {
                    if scanner.peek() == Some('-') && scanner.peek_at(1) == Some('}') {
                        scanner.bump();
                        scanner.bump();
                        break;
                    }
                    scanner.bump();
                }
            }
            '"' => {
                scanner.bump();
                match lex_string(&mut scanner) {
                    Ok(kind) => {
                        seen_token = true;
                        tokens.push(RawToken::new(kind, start..scanner.position()));
                    }
                    Err((span, message)) => errors.push(LexError { span, message }),
                }
            }
            '\'' => match lex_char(&mut scanner) {
                Ok(kind) => {
                    seen_token = true;
                    tokens.push(RawToken::new(kind, start..scanner.position()));
                }
                Err((span, message)) => errors.push(LexError { span, message }),
            },
            '(' => simple(
                &mut scanner,
                &mut tokens,
                &mut seen_token,
                RawTokenKind::LParen,
            ),
            ')' => simple(
                &mut scanner,
                &mut tokens,
                &mut seen_token,
                RawTokenKind::RParen,
            ),
            '{' => simple(
                &mut scanner,
                &mut tokens,
                &mut seen_token,
                RawTokenKind::LBrace,
            ),
            '}' => simple(
                &mut scanner,
                &mut tokens,
                &mut seen_token,
                RawTokenKind::RBrace,
            ),
            '[' => simple(
                &mut scanner,
                &mut tokens,
                &mut seen_token,
                RawTokenKind::LBracket,
            ),
            ']' => simple(
                &mut scanner,
                &mut tokens,
                &mut seen_token,
                RawTokenKind::RBracket,
            ),
            ',' => simple(
                &mut scanner,
                &mut tokens,
                &mut seen_token,
                RawTokenKind::Comma,
            ),
            '`' => simple(
                &mut scanner,
                &mut tokens,
                &mut seen_token,
                RawTokenKind::Backtick,
            ),
            '\\' if !scanner.peek_at(1).is_some_and(is_symbol_char) => simple(
                &mut scanner,
                &mut tokens,
                &mut seen_token,
                RawTokenKind::Backslash,
            ),
            '?' if scanner.peek_at(1).is_some_and(is_identifier_char) => {
                scanner.bump();
                let (_, name) = scanner.consume_while(is_identifier_char);
                seen_token = true;
                tokens.push(RawToken::new(
                    RawTokenKind::Hole(name),
                    start..scanner.position(),
                ));
            }
            c if c.is_ascii_digit() => match lex_number(&mut scanner) {
                Ok(kind) => {
                    seen_token = true;
                    tokens.push(RawToken::new(kind, start..scanner.position()));
                }
                Err((span, message)) => errors.push(LexError { span, message }),
            },
            c if is_uppercase_start(c) => {
                let (_, text) = scanner.consume_while(is_identifier_char);
                seen_token = true;
                tokens.push(RawToken::new(
                    RawTokenKind::UpperIdent(text),
                    start..scanner.position(),
                ));
            }
            c if is_identifier_start(c) => {
                let (_, text) = scanner.consume_while(is_identifier_char);
                seen_token = true;
                tokens.push(RawToken::new(keyword(&text), start..scanner.position()));
            }
            c if is_symbol_char(c) => {
                let (_, text) = scanner.consume_while(is_symbol_char);
                seen_token = true;
                tokens.push(RawToken::new(
                    classify_operator(&text),
                    start..scanner.position(),
                ));
            }
            other => {
                scanner.bump();
                errors.push(LexError {
                    span: TextRange::new(start as u32, scanner.position() as u32),
                    message: format!("unexpected character {other:?}"),
                });
            }
        }
    }
    tokens.push(RawToken {
        kind: RawTokenKind::Eof,
        span: TextRange::empty(source.len() as u32),
    });
    (tokens, errors)
}

fn simple(
    scanner: &mut Scanner,
    tokens: &mut Vec<RawToken>,
    seen_token: &mut bool,
    kind: RawTokenKind,
) {
    let start = scanner.position();
    scanner.bump();
    *seen_token = true;
    tokens.push(RawToken::new(kind, start..scanner.position()));
}

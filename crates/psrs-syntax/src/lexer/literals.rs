use super::RawTokenKind;
use super::scanner::Scanner;
use psrs_span::TextRange;

pub(super) fn lex_number(scanner: &mut Scanner) -> Result<RawTokenKind, (TextRange, String)> {
    let start = scanner.position();
    if scanner.peek() == Some('0') && matches!(scanner.peek_at(1), Some('x' | 'X')) {
        scanner.bump();
        scanner.bump();
        let (_, digits) = scanner.consume_while(|c| c.is_ascii_hexdigit() || c == '_');
        if digits.chars().all(|c| c == '_') {
            return Err(error_at(
                start,
                scanner.position(),
                "expected hexadecimal digits",
            ));
        }
        return Ok(RawTokenKind::Integer(format!(
            "0x{}",
            digits.replace('_', "")
        )));
    }
    let (_, integer) = scanner.consume_while(|c| c.is_ascii_digit() || c == '_');
    if integer.len() > 1 && integer.starts_with('0') {
        return Err(error_at(
            start,
            scanner.position(),
            "leading zeros are not allowed",
        ));
    }
    let mut text = integer.replace('_', "");
    let mut is_number = false;
    if scanner.peek() == Some('.')
        && scanner.peek_at(1) != Some('.')
        && scanner.peek_at(1).is_some_and(|c| c.is_ascii_digit())
    {
        is_number = true;
        scanner.bump();
        let (_, fraction) = scanner.consume_while(|c| c.is_ascii_digit() || c == '_');
        text.push('.');
        text.push_str(&fraction.replace('_', ""));
    }
    if matches!(scanner.peek(), Some('e' | 'E')) {
        let checkpoint = scanner.cursor;
        scanner.bump();
        let sign = if matches!(scanner.peek(), Some('+' | '-')) {
            scanner.bump()
        } else {
            None
        };
        let (_, exponent) = scanner.consume_while(|c| c.is_ascii_digit());
        if exponent.is_empty() {
            scanner.cursor = checkpoint;
        } else {
            is_number = true;
            text.push('e');
            if let Some(sign) = sign {
                text.push(sign);
            }
            text.push_str(&exponent);
        }
    }
    if is_number {
        Ok(RawTokenKind::Number(text))
    } else {
        Ok(RawTokenKind::Integer(text))
    }
}

/// One decoded escape: the code point it denotes and the range it covers. The
/// code point is kept as a `u32` because a `\x` escape may denote a surrogate,
/// which is not a Unicode scalar value until it is paired or rejected.
pub(super) struct Escape {
    pub(super) code_point: u32,
    pub(super) span: TextRange,
}

fn escape(start: usize, code_point: u32, end: usize) -> Escape {
    Escape {
        code_point,
        span: TextRange::new(start as u32, end as u32),
    }
}

pub(super) fn lex_escape(scanner: &mut Scanner) -> Result<Escape, (TextRange, String)> {
    let start = scanner.position();
    match scanner.peek() {
        Some('t') => {
            scanner.bump();
            Ok(escape(start, u32::from('\t'), scanner.position()))
        }
        Some('r') => {
            scanner.bump();
            Ok(escape(start, u32::from('\r'), scanner.position()))
        }
        Some('n') => {
            scanner.bump();
            Ok(escape(start, u32::from('\n'), scanner.position()))
        }
        Some('"') => {
            scanner.bump();
            Ok(escape(start, u32::from('"'), scanner.position()))
        }
        Some('\'') => {
            scanner.bump();
            Ok(escape(start, u32::from('\''), scanner.position()))
        }
        Some('\\') => {
            scanner.bump();
            Ok(escape(start, u32::from('\\'), scanner.position()))
        }
        Some('x') => {
            scanner.bump();
            let mut digits = String::new();
            while digits.len() < 6 {
                match scanner.peek() {
                    Some(c) if c.is_ascii_hexdigit() => {
                        digits.push(c);
                        scanner.bump();
                    }
                    _ => break,
                }
            }
            let code_point = u32::from_str_radix(&digits, 16).unwrap_or(0);
            Ok(escape(start, code_point, scanner.position()))
        }
        _ => Err(error_at(start, scanner.position() + 1, "invalid escape")),
    }
}

pub(super) fn is_high_surrogate(code_point: u32) -> bool {
    (0xD800..=0xDBFF).contains(&code_point)
}

pub(super) fn is_low_surrogate(code_point: u32) -> bool {
    (0xDC00..=0xDFFF).contains(&code_point)
}

/// Joins an escaped surrogate pair into the scalar value it denotes.
pub(super) fn combine_surrogates(high: u32, low: u32) -> u32 {
    0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
}

/// Appends one escape to a string literal's scalar sequence. A contiguous
/// escaped high-surrogate/low-surrogate pair decodes as its one scalar value;
/// an unpaired surrogate is rejected rather than replaced.
fn push_escape(
    decoded: &mut String,
    escape: Escape,
    scanner: &mut Scanner,
) -> Result<(), (TextRange, String)> {
    let unpaired = (
        escape.span,
        "unpaired surrogate in string literal".to_owned(),
    );
    if is_low_surrogate(escape.code_point) {
        return Err(unpaired);
    }
    if is_high_surrogate(escape.code_point) {
        // The pair must be contiguous: `\` `x` and the low surrogate follow the
        // high surrogate with nothing between them.
        if scanner.peek() != Some('\\') {
            return Err(unpaired);
        }
        let checkpoint = scanner.cursor;
        scanner.bump();
        let low = match lex_escape(scanner) {
            Ok(next) if is_low_surrogate(next.code_point) => next,
            _ => {
                scanner.cursor = checkpoint;
                return Err(unpaired);
            }
        };
        decoded.push(
            char::from_u32(combine_surrogates(escape.code_point, low.code_point))
                .expect("a surrogate pair denotes one scalar value"),
        );
        return Ok(());
    }
    match char::from_u32(escape.code_point) {
        Some(character) => {
            decoded.push(character);
            Ok(())
        }
        None => Err((
            escape.span,
            "escape does not denote a Unicode scalar value".to_owned(),
        )),
    }
}

pub(super) fn lex_string(scanner: &mut Scanner) -> Result<RawTokenKind, (TextRange, String)> {
    let mut quotes = 1usize;
    while scanner.peek() == Some('"') && quotes < 8 {
        scanner.bump();
        quotes += 1;
    }
    match quotes {
        1 => {
            let mut decoded = String::new();
            loop {
                match scanner.peek() {
                    Some('"') => {
                        scanner.bump();
                        break;
                    }
                    Some('\\') => {
                        scanner.bump();
                        if scanner
                            .peek()
                            .is_some_and(|c| c == ' ' || c == '\t' || c == '\r' || c == '\n')
                        {
                            while scanner
                                .peek()
                                .is_some_and(|c| c == ' ' || c == '\t' || c == '\r' || c == '\n')
                            {
                                scanner.cursor += 1;
                            }
                            match scanner.peek() {
                                Some('\\') => {
                                    scanner.bump();
                                }
                                Some('"') | None => {}
                                Some(other) => {
                                    let at = scanner.position();
                                    return Err(error_at(
                                        at,
                                        at + other.len_utf8(),
                                        "character in string gap",
                                    ));
                                }
                            }
                        } else {
                            let escape = lex_escape(scanner)?;
                            push_escape(&mut decoded, escape, scanner)?;
                        }
                    }
                    Some('\n') | Some('\r') | None => {
                        let at = scanner.position();
                        return Err(error_at(at, at, "unterminated string"));
                    }
                    Some(character) => {
                        decoded.push(character);
                        scanner.bump();
                    }
                }
            }
            Ok(RawTokenKind::String(decoded))
        }
        2 => Ok(RawTokenKind::String(String::new())),
        count if count >= 6 => Ok(RawTokenKind::String("\"".repeat(count - 6))),
        _ => {
            let mut content = String::new();
            for _ in 0..quotes.saturating_sub(3) {
                content.push('"');
            }
            loop {
                let (_, plain) = scanner.consume_while(|c| c != '"');
                content.push_str(&plain);
                let mut run = 0usize;
                while scanner.peek() == Some('"') && run < 5 {
                    scanner.bump();
                    run += 1;
                }
                match run {
                    0 => {
                        let at = scanner.position();
                        return Err(error_at(at, at, "unterminated raw string"));
                    }
                    n if n >= 3 => {
                        for _ in 0..n - 3 {
                            content.push('"');
                        }
                        break;
                    }
                    n => {
                        for _ in 0..n {
                            content.push('"');
                        }
                    }
                }
            }
            Ok(RawTokenKind::String(content))
        }
    }
}

/// A character literal is exactly one Unicode scalar value, supplementary
/// included. A `\x` escape denoting a surrogate has no partner inside one
/// character, so it is rejected.
pub(super) fn lex_char(scanner: &mut Scanner) -> Result<RawTokenKind, (TextRange, String)> {
    let start = scanner.position();
    scanner.bump();
    let character = if scanner.peek() == Some('\\') {
        scanner.bump();
        let escape = lex_escape(scanner)?;
        if is_high_surrogate(escape.code_point) || is_low_surrogate(escape.code_point) {
            return Err((
                escape.span,
                "unpaired surrogate in character literal".to_owned(),
            ));
        }
        match char::from_u32(escape.code_point) {
            Some(character) => character,
            None => {
                return Err((
                    escape.span,
                    "escape does not denote a Unicode scalar value".to_owned(),
                ));
            }
        }
    } else {
        match scanner.bump() {
            Some(character) => character,
            None => return Err(error_at(start, start, "unterminated character")),
        }
    };
    if scanner.bump() != Some('\'') {
        let at = scanner.position();
        return Err(error_at(at, at, "unterminated character"));
    }
    Ok(RawTokenKind::Char(character))
}

pub(super) fn error_at(start: usize, end: usize, message: &str) -> (TextRange, String) {
    (TextRange::new(start as u32, end as u32), message.to_owned())
}

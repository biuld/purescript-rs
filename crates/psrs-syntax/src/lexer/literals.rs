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

pub(super) fn lex_escape(scanner: &mut Scanner) -> Result<(String, char), (TextRange, String)> {
    let start = scanner.position();
    match scanner.peek() {
        Some('t') => {
            scanner.bump();
            Ok(("t".into(), '\t'))
        }
        Some('r') => {
            scanner.bump();
            Ok(("r".into(), '\r'))
        }
        Some('n') => {
            scanner.bump();
            Ok(("n".into(), '\n'))
        }
        Some('"') => {
            scanner.bump();
            Ok(("\"".into(), '"'))
        }
        Some('\'') => {
            scanner.bump();
            Ok(("'".into(), '\''))
        }
        Some('\\') => {
            scanner.bump();
            Ok(("\\".into(), '\\'))
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
            let value = u32::from_str_radix(&digits, 16).unwrap_or(0);
            let character = char::from_u32(value).unwrap_or(char::REPLACEMENT_CHARACTER);
            Ok((format!("x{digits}"), character))
        }
        _ => Err(error_at(start, scanner.position() + 1, "invalid escape")),
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
                            let (_, character) = lex_escape(scanner)?;
                            decoded.push(character);
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

pub(super) fn lex_char(scanner: &mut Scanner) -> Result<RawTokenKind, (TextRange, String)> {
    let start = scanner.position();
    scanner.bump();
    let character = if scanner.peek() == Some('\\') {
        scanner.bump();
        lex_escape(scanner)?.1
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
    // A Rust `char` is already a Unicode scalar, including U+10000..=U+10FFFF.
    Ok(RawTokenKind::Char(character))
}

pub(super) fn error_at(start: usize, end: usize, message: &str) -> (TextRange, String) {
    (TextRange::new(start as u32, end as u32), message.to_owned())
}

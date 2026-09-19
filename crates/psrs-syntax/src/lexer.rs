use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawToken {
    pub kind: RawTokenKind,
    pub span: TextRange,
}

impl RawToken {
    fn new(kind: RawTokenKind, span: std::ops::Range<usize>) -> Self {
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
    Number(String),
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
    Foreign,
    Infix,
    Infixl,
    Infixr,
    Role,
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

    Hole(String),

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    DotDot,
    Colon,
    DoubleColon,
    Equals,
    FatArrow,
    Arrow,
    LeftArrow,
    Pipe,
    Backslash,
    Backtick,
    Newline,
    Eof,
}

impl RawTokenKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::LowerIdent(_) => "LowerIdent",
            Self::UpperIdent(_) => "UpperIdent",
            Self::Integer(_) => "Integer",
            Self::Number(_) => "Number",
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
            Self::Foreign => "Foreign",
            Self::Infix => "Infix",
            Self::Infixl => "Infixl",
            Self::Infixr => "Infixr",
            Self::Role => "Role",
            Self::Hole(_) => "Hole",
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
            Self::DotDot => "DotDot",
            Self::Colon => "Colon",
            Self::DoubleColon => "DoubleColon",
            Self::Equals => "Equals",
            Self::FatArrow => "FatArrow",
            Self::Arrow => "Arrow",
            Self::LeftArrow => "LeftArrow",
            Self::Pipe => "Pipe",
            Self::Backslash => "Backslash",
            Self::Backtick => "Backtick",
            Self::Newline => "Newline",
            Self::Eof => "Eof",
        }
    }

    pub fn source_text(&self) -> Option<&str> {
        match self {
            Self::LowerIdent(text)
            | Self::UpperIdent(text)
            | Self::Integer(text)
            | Self::Number(text)
            | Self::Operator(text)
            | Self::Hole(text) => Some(text),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    pub span: TextRange,
    pub message: String,
}

fn keyword(text: &str) -> RawTokenKind {
    match text {
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
        "foreign" => RawTokenKind::Foreign,
        "infix" => RawTokenKind::Infix,
        "infixl" => RawTokenKind::Infixl,
        "infixr" => RawTokenKind::Infixr,
        "role" => RawTokenKind::Role,
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
        _ => RawTokenKind::LowerIdent(text.to_owned()),
    }
}

fn is_symbol_char(character: char) -> bool {
    if character.is_ascii() {
        "!#$%&*+./<=>?@\\^|-~:".contains(character)
    } else {
        !character.is_alphanumeric() && !character.is_whitespace() && !character.is_control()
    }
}

fn is_identifier_start(character: char) -> bool {
    character.is_lowercase() || character == '_'
}

fn is_identifier_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '\''
}

fn is_uppercase_start(character: char) -> bool {
    character.is_uppercase()
}

struct Scanner {
    chars: Vec<(usize, char)>,
    cursor: usize,
    len: u32,
}

impl Scanner {
    fn new(source: &str) -> Self {
        Self {
            chars: source.char_indices().collect(),
            cursor: 0,
            len: source.len() as u32,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.cursor).map(|(_, c)| *c)
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.cursor + offset).map(|(_, c)| *c)
    }

    fn position(&self) -> usize {
        self.chars
            .get(self.cursor)
            .map(|(byte, _)| *byte)
            .unwrap_or(self.len as usize)
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek();
        if character.is_some() {
            self.cursor += 1;
        }
        character
    }

    fn consume_while(&mut self, predicate: impl Fn(char) -> bool) -> (usize, String) {
        let start = self.position();
        let mut text = String::new();
        while let Some(character) = self.peek() {
            if !predicate(character) {
                break;
            }
            text.push(character);
            self.cursor += 1;
        }
        (start, text)
    }
}

fn classify_operator(text: &str) -> RawTokenKind {
    match text {
        "=" => RawTokenKind::Equals,
        "::" | "∷" => RawTokenKind::DoubleColon,
        "=>" | "⇒" => RawTokenKind::FatArrow,
        "->" | "→" => RawTokenKind::Arrow,
        "<-" | "←" => RawTokenKind::LeftArrow,
        "|" => RawTokenKind::Pipe,
        "." => RawTokenKind::Dot,
        ".." => RawTokenKind::DotDot,
        ":" => RawTokenKind::Colon,
        "∀" => RawTokenKind::Forall,
        _ => RawTokenKind::Operator(text.to_owned()),
    }
}

fn lex_number(scanner: &mut Scanner) -> Result<RawTokenKind, (TextRange, String)> {
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

fn lex_escape(scanner: &mut Scanner) -> Result<(String, char), (TextRange, String)> {
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

fn lex_string(scanner: &mut Scanner) -> Result<RawTokenKind, (TextRange, String)> {
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

fn lex_char(scanner: &mut Scanner) -> Result<RawTokenKind, (TextRange, String)> {
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
    if character as u32 > 0xFFFF {
        return Err(error_at(
            start,
            scanner.position(),
            "astral code point in character literal",
        ));
    }
    Ok(RawTokenKind::Char(character))
}

fn error_at(start: usize, end: usize, message: &str) -> (TextRange, String) {
    (TextRange::new(start as u32, end as u32), message.to_owned())
}

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
        let (_, errors) = lex("main = \u{1}");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].span, TextRange::new(7, 8));
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

    #[test]
    fn lexes_unicode_identifiers_and_operators() {
        let (tokens, errors) = lex("f asgård = 1 ∘ 2\n");
        assert!(errors.is_empty(), "{errors:?}");
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == RawTokenKind::LowerIdent("asgård".into()))
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == RawTokenKind::Operator("∘".into()))
        );
    }

    #[test]
    fn lexes_hex_escapes_and_block_strings() {
        let (tokens, errors) = lex(r#"main = "\x1D306""#);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == RawTokenKind::String("𝌆".into()))
        );

        let (tokens, errors) = lex("main = \"\"\"foo\"\"\"\n");
        assert!(errors.is_empty(), "{errors:?}");
        assert!(
            tokens
                .iter()
                .any(|token| token.kind == RawTokenKind::String("foo".into()))
        );
    }
}

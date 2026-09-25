use super::RawTokenKind;

pub(super) fn keyword(text: &str) -> RawTokenKind {
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

pub(super) fn is_symbol_char(character: char) -> bool {
    if character.is_ascii() {
        "!#$%&*+./<=>?@\\^|-~:".contains(character)
    } else {
        !character.is_alphanumeric() && !character.is_whitespace() && !character.is_control()
    }
}

pub(super) fn is_identifier_start(character: char) -> bool {
    character.is_lowercase() || character == '_'
}

pub(super) fn is_identifier_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '\''
}

pub(super) fn is_uppercase_start(character: char) -> bool {
    character.is_uppercase()
}

pub(super) struct Scanner {
    pub(super) chars: Vec<(usize, char)>,
    pub(super) cursor: usize,
    pub(super) len: u32,
}

impl Scanner {
    pub(super) fn new(source: &str) -> Self {
        Self {
            chars: source.char_indices().collect(),
            cursor: 0,
            len: source.len() as u32,
        }
    }

    pub(super) fn peek(&self) -> Option<char> {
        self.chars.get(self.cursor).map(|(_, c)| *c)
    }

    pub(super) fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.cursor + offset).map(|(_, c)| *c)
    }

    pub(super) fn position(&self) -> usize {
        self.chars
            .get(self.cursor)
            .map(|(byte, _)| *byte)
            .unwrap_or(self.len as usize)
    }

    pub(super) fn bump(&mut self) -> Option<char> {
        let character = self.peek();
        if character.is_some() {
            self.cursor += 1;
        }
        character
    }

    pub(super) fn consume_while(&mut self, predicate: impl Fn(char) -> bool) -> (usize, String) {
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

pub(super) fn classify_operator(text: &str) -> RawTokenKind {
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

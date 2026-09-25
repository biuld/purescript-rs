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

mod literals;
mod run;
mod scanner;

pub use run::lex;

#[cfg(test)]
mod tests;

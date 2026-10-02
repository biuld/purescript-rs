use crate::TypeId;
use psrs_hir::{LocalId, SymbolId};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub ty: TypeId,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Var {
        id: LocalId,
        ty: TypeId,
    },
    Literal {
        value: Literal,
    },
    Array {
        elements: Vec<Pattern>,
    },
    Named {
        id: LocalId,
        pattern: Box<Pattern>,
    },
    /// A constructor pattern, with one nested pattern per field.
    Constructor {
        symbol: SymbolId,
        arguments: Vec<Pattern>,
    },
    Record {
        fields: Vec<(String, Pattern)>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Literal {
    Integer(i32),
    Number(String),
    String(String),
    Char(char),
    Boolean(bool),
}

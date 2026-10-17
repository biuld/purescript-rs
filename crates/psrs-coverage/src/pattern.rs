//! The neutral pattern algebra of coverage analysis.
//!
//! A shared [`Pattern`] is a binder-erased shape: alternatives erase their
//! variables at their owning adapter before entering analysis, so pattern
//! equality is shape equality and no stage reconstructs binder facts. Its type
//! identity `Ty` is the supplying stage's own identity — the type checker's
//! checked type ids and the core module's type-table indexes are both valid,
//! and the engine only ever compares or carries them.

use psrs_hir::SymbolId;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Pattern<Ty> {
    Any {
        ty: Ty,
    },
    Literal {
        literal: Literal,
        ty: Ty,
    },
    Array {
        elements: Vec<Pattern<Ty>>,
        ty: Ty,
    },
    Constructor {
        symbol: SymbolId,
        arguments: Vec<Pattern<Ty>>,
        ty: Ty,
    },
    Record {
        fields: Vec<(String, Pattern<Ty>)>,
        ty: Ty,
    },
}

impl<Ty: Copy> Pattern<Ty> {
    /// The type the pattern constrains.
    pub fn pattern_type(&self) -> Ty {
        match self {
            Pattern::Any { ty }
            | Pattern::Literal { ty, .. }
            | Pattern::Array { ty, .. }
            | Pattern::Constructor { ty, .. }
            | Pattern::Record { ty, .. } => *ty,
        }
    }
}

/// The neutral projection of a scalar alternative. Both consumers' native
/// literal representations map onto this one, and canonicalization equates
/// spellings the official linter's `missingCases` treats identically.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Literal {
    Integer(i32),
    Number(String),
    String(String),
    Char(char),
    Boolean(bool),
}

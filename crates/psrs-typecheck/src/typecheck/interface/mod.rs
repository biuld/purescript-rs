//! Closed, immutable value interfaces published only by successful P5 producers.

use super::*;
use psrs_kind::ClosedKinds;

mod freeze;
mod instantiate;

/// A checked value's source-level semantic type, before dictionary erasure.
/// All type identities inside this opaque value are lexical binders, and kinds
/// have their own closed scope. No inference state is retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedValueSignature {
    source_span: TextRange,
    name_span: Option<TextRange>,
    kinds: ClosedKinds,
    ty: ClosedType,
}

impl CheckedValueSignature {
    pub fn source_span(&self) -> TextRange {
        self.source_span
    }
    pub fn name_span(&self) -> Option<TextRange> {
        self.name_span
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ClosedType {
    Variable(u32),
    Constructor(TypeConstructor),
    Application(Box<ClosedType>, Box<ClosedType>),
    ForAll {
        binders: Vec<ClosedBinder>,
        body: Box<ClosedType>,
    },
    Constrained {
        constraints: Vec<ClosedConstraint>,
        body: Box<ClosedType>,
    },
    RowEmpty,
    RowExtend {
        label: String,
        ty: Box<ClosedType>,
        tail: Box<ClosedType>,
    },
    String(String),
    Integer(i64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClosedBinder {
    variable: u32,
    visible: bool,
    kind: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClosedConstraint {
    class_id: hir::TypeId,
    arguments: Vec<ClosedType>,
    span: TextRange,
}

#[cfg(test)]
mod tests;

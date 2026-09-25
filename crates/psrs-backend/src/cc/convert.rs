//! Target-neutral conversion plans for values whose runtime layouts differ.

use super::{ReprId, ValueShape};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoxKind {
    Integer,
    Number,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RecoveryEvidence {
    TypeInstantiation,
    ErasedVariantField {
        variant: ReprId,
        tag: u32,
        field: u32,
        template: ValueShape,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ValueConversion {
    Identity,
    BoxScalar {
        kind: BoxKind,
        representation: ReprId,
    },
    UnboxScalar {
        kind: BoxKind,
        representation: ReprId,
        destination: ValueShape,
    },
    EraseReference,
    RecoverReference {
        destination: ValueShape,
        evidence: RecoveryEvidence,
    },
    Sequence(Vec<ValueConversion>),
    ArrayMap {
        source: ReprId,
        target: ReprId,
        element: Box<ValueConversion>,
    },
    ProductMap {
        source: ReprId,
        target: ReprId,
        labels: Vec<String>,
        fields: Vec<ValueConversion>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AggregateConvert {
    pub source: ValueShape,
    pub destination: ValueShape,
    pub plan: ValueConversion,
}

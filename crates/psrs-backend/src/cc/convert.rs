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
    /// Calls a P8-generated factory that captures the source closure and returns
    /// a closure with the destination calling convention. Nested maps use the
    /// same operation as scalar function boundaries.
    FunctionAdapter {
        function: psrs_hir::SymbolId,
        source: ValueShape,
        destination: ValueShape,
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

impl ValueConversion {
    /// The declared output of a conversion plan. This does not validate its
    /// input or representation metadata; CC verification discharges those checks.
    pub(crate) fn output_shape(&self, source: ValueShape) -> ValueShape {
        let reference = |id| {
            ValueShape::Reference(super::Reference {
                nullable: false,
                heap: super::RefShape::Repr(id),
            })
        };
        match self {
            Self::Identity => source,
            Self::BoxScalar { representation, .. } => reference(*representation),
            Self::UnboxScalar { destination, .. }
            | Self::RecoverReference { destination, .. }
            | Self::FunctionAdapter { destination, .. } => *destination,
            Self::EraseReference => super::payload::erased_shape(),
            Self::ArrayMap { target, .. } | Self::ProductMap { target, .. } => reference(*target),
            Self::Sequence(steps) => steps
                .iter()
                .fold(source, |shape, step| step.output_shape(shape)),
        }
    }
}

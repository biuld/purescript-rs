use super::TypeId;
use psrs_hir::{LocalId, SymbolId, TypeId as ClassId};
use psrs_span::TextRange;

/// A frontend-selected dictionary derivation retained in Typed Core.
///
/// The class solver owns the meaning and coherence of this derivation. Core
/// lowering consumes it into ordinary local/global values, calls, and record
/// projections before CC sees the program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub kind: EvidenceKind,
    /// The source class constraint this term proves. This identity is erased
    /// with the evidence node and is never a runtime type tag.
    pub class_id: ClassId,
    /// The dictionary record type proved by this evidence.
    pub ty: TypeId,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvidenceKind {
    /// A dictionary parameter introduced by a constrained binding.
    Given(LocalId),
    /// A dictionary value already bound by the class elaborator.
    Global(SymbolId),
    /// A dictionary obtained from a superclass field of another dictionary.
    Superclass {
        parent: Box<Evidence>,
        field: String,
    },
    /// A selected instance dictionary constructor applied to its context
    /// evidence. The constructor's type is explicit so the verifier can check
    /// each application without consulting the frontend's instance table.
    Instance {
        constructor: SymbolId,
        constructor_type: TypeId,
        context: Vec<Evidence>,
    },
}

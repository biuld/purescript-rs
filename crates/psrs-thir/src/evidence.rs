use super::TypeId;
use psrs_hir::{LocalId, SymbolId, TypeId as ClassId};
use psrs_span::TextRange;

/// Authority for a conversion that is not an ordinary Coercible proof.
/// Newtype deriving relies on representation transparency and a selected
/// wrapped instance, as upstream dictionary reuse does. The checker owns that
/// validation; THIR verifies the named newtype and the conversion endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UncheckedCoercionOrigin {
    UnsafeCoerce,
    NewtypeDeriving {
        class_id: ClassId,
        newtype_id: ClassId,
    },
}

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
    /// A compiler-constructed dictionary expressed as checked ordinary terms.
    /// The verifier checks its fields, lexical scope, and dictionary type;
    /// the frontend owns the class rule that authorizes construction.
    DictionaryValue(Box<super::Expr>),
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
    /// A compiler-derived `Prim.Coerce.Coercible` proof with the checked types
    /// at both ends. This is compile-time evidence, not a runtime dictionary.
    Coercible {
        source_type: TypeId,
        target_type: TypeId,
    },
    /// A `Prim` relation's dictionary: an ordinary dictionary node that erases,
    /// carrying the arguments the rule decided rather than a constructor to
    /// apply. A `Prim` relation has no members, so its dictionary is empty and
    /// the decision it made is what the evidence records.
    Primitive { arguments: Vec<TypeId> },
}

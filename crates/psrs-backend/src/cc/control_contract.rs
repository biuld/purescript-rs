//! Immutable source contracts checked by Core and quoted before representation
//! erasure. No Core arena indices remain in the published CC evidence.

use super::{ReprId, SignatureId};
use psrs_hir::{Intrinsic, TypeId, TypeVariableId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ControlContractId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceTypeIdentityId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeConstructorIdentity {
    Function,
    Record,
    Row,
    Array,
    Int,
    Number,
    Boolean,
    String,
    Char,
    Unit,
    Type,
    Constraint,
    Symbol,
    User(TypeId),
}

/// Lexical binders and stable free-variable identities are retained, including
/// nested quantifiers and open rows. This is evidence, not a second type solver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeIdentity {
    Variable(TypeVariableId),
    Constructor(TypeConstructorIdentity),
    Application(Box<TypeIdentity>, Box<TypeIdentity>),
    ForAll {
        variables: Vec<TypeVariableId>,
        body: Box<TypeIdentity>,
    },
    Closure {
        parameters: Vec<TypeIdentity>,
        result: Box<TypeIdentity>,
    },
    RowEmpty,
    RowExtend {
        label: String,
        ty: Box<TypeIdentity>,
        tail: Box<TypeIdentity>,
    },
    TypeLevelString(String),
    TypeLevelInt(i64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlDomain {
    pub request: TypeIdentity,
    pub answer: TypeIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ControlImplementation {
    Frame {
        representation: ReprId,
        case: u32,
    },
    Call {
        signature: SignatureId,
        role: super::ControlHelperRole,
    },
}

/// A source use can only be published from the control owner's checked proof.
/// Its fields are immutable to callers constructing or inspecting CC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlContract {
    pub(super) implementation: ControlImplementation,
    pub(super) result_identity: SourceTypeIdentityId,
    pub(super) argument_identities: Vec<SourceTypeIdentityId>,
    pub(super) operation: Intrinsic,
    pub(super) result: TypeIdentity,
    pub(super) arguments: Vec<TypeIdentity>,
    pub(super) source: Option<ControlDomain>,
    pub(super) target: Option<ControlDomain>,
}

impl ControlContract {
    pub fn result_identity(&self) -> SourceTypeIdentityId {
        self.result_identity
    }
    pub fn argument_identities(&self) -> &[SourceTypeIdentityId] {
        &self.argument_identities
    }
    pub fn operation(&self) -> Intrinsic {
        self.operation
    }
    pub fn result(&self) -> &TypeIdentity {
        &self.result
    }
    pub fn arguments(&self) -> &[TypeIdentity] {
        &self.arguments
    }
    pub fn source(&self) -> Option<&ControlDomain> {
        self.source.as_ref()
    }
    pub fn target(&self) -> Option<&ControlDomain> {
        self.target.as_ref()
    }
}

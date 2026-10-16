//! The vocabulary shared by every part of inference: the scheme and constraint
//! forms inference produces, and the environment entries the semantic table
//! holds. Nothing here is mutable solver state; the fields live in
//! [`SemanticEnv`](super::state::SemanticEnv), [`InferState`](super::state::InferState),
//! and [`Scope`](super::state::Scope).

use super::*;

/// A type with a set of universally quantified variables and the class
/// constraints those variables must satisfy.
///
/// `variable_kinds` is the kind of each quantified variable, read from the kind
/// layer at the place the variable was bound rather than re-derived here. An
/// instantiation records it on the fresh variable it allocates, so the
/// polymorphism a scheme carries is self-contained: it does not depend on the
/// solver table that happened to allocate the original variables.
#[derive(Clone, Debug)]
pub(super) struct Scheme {
    pub(super) variables: Vec<u32>,
    pub(super) variable_kinds: HashMap<u32, Kind>,
    pub(super) constraints: Vec<ClassConstraint>,
    pub(super) ty: InferType,
}

impl Scheme {
    pub(super) fn monomorphic(ty: InferType) -> Self {
        Self {
            variables: Vec::new(),
            variable_kinds: HashMap::new(),
            constraints: Vec::new(),
            ty,
        }
    }
}

/// A class constraint `C τ...` recorded during inference. Its `arguments` are
/// the instantiated class type arguments in declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ClassConstraint {
    pub(super) class_id: hir::TypeId,
    pub(super) arguments: Vec<InferType>,
    pub(super) span: TextRange,
}

/// A class method declaration recorded in the class environment.
#[derive(Clone, Debug)]
pub(super) struct MethodInfo {
    pub(super) symbol: SymbolId,
    pub(super) name: String,
    pub(super) signature: hir::Type,
}

/// A type written over an explicit binder scope. `binders` are the inference
/// variables the body is written in, in binding order, and `body` is the
/// template's spine. Instantiation substitutes a caller's arguments for the
/// binders through the shared `substitute`, so a template and the constraint it
/// denotes are one representation rather than two that can disagree.
#[derive(Clone, Debug)]
pub(super) struct TypeTemplate {
    pub(super) binders: Vec<u32>,
    pub(super) body: Vec<InferType>,
}

/// One superclass edge of a class. The edge's arguments are written over the
/// subclass's own parameters and kept as a `TypeTemplate`; instantiating that
/// template with a subclass's arguments yields the superclass constraint, which
/// is the only way the edge's arguments are ever read. `field` is the dictionary
/// record field that stores the superclass dictionary, chosen from the edge's
/// position.
#[derive(Clone, Debug)]
pub(super) struct SuperclassInfo {
    pub(super) class_id: hir::TypeId,
    pub(super) template: TypeTemplate,
    pub(super) field: String,
    pub(super) span: TextRange,
}

/// One functional dependency of a class, resolved to parameter positions. The
/// `determining` parameters functionally determine the `determined` ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FundepInfo {
    pub(super) determining: Vec<usize>,
    pub(super) determined: Vec<usize>,
}

/// A class with its ordered type parameters, superclass edges, methods, and
/// functional dependencies.
#[derive(Clone, Debug)]
pub(super) struct ClassInfo {
    pub(super) compiler_class: Option<hir::CompilerClass>,
    pub(super) parameters: Vec<String>,
    pub(super) superclasses: Vec<SuperclassInfo>,
    pub(super) fundeps: Vec<FundepInfo>,
    pub(super) methods: Vec<MethodInfo>,
}

/// An instance's class, head arguments (which may contain instance variables),
/// elaborated context constraints, and the synthesized dictionary parameters
/// for that context.
#[derive(Clone, Debug)]
pub(super) struct InstanceInfo {
    pub(super) symbol: SymbolId,
    pub(super) class_id: hir::TypeId,
    pub(super) chain_id: u32,
    pub(super) chain_position: u32,
    pub(super) head_arguments: Vec<InferType>,
    /// Every type variable shared by the instance head and context. Context
    /// variables determined through fundeps still need one identity in method
    /// bodies and their type annotations.
    pub(super) instance_variables: HashMap<String, InferType>,
    pub(super) context: Vec<ClassConstraint>,
    pub(super) context_parameters: Vec<(LocalId, InferType)>,
}

/// The dictionary selected for a wanted constraint during solving. A
/// superclass selection embeds the parent constraint's already-solved
/// dictionary, and an instance selection embeds the solved context constraints
/// whose dictionaries it applies the constructor to.
#[derive(Clone, Debug)]
pub(super) enum WantedSolution {
    /// A compiler-constructed dictionary with checked ordinary term fields.
    DictionaryValue(Box<InferredExpr>),
    Given(LocalId),
    Global(SymbolId),
    Instance {
        constructor: SymbolId,
        constructor_type: InferType,
        context: Vec<u32>,
    },
    Superclass {
        parent: Box<WantedConstraint>,
        field: String,
    },
    /// A dictionary parameter the declaration abstracts for a constraint it could
    /// not discharge. The constraint stays in the declaration's scheme, and the
    /// body's evidence is this parameter, so the body and the scheme name one
    /// dictionary rather than two elaborations of the same class.
    Abstracted(LocalId),
    Coercible {
        source: InferType,
        target: InferType,
    },
    /// A `Prim` relation's dictionary: an ordinary dictionary that erases when
    /// the relation is only about types, recording the arguments the rule
    /// decided. The decision is the evidence; the runtime value is empty.
    Primitive {
        arguments: Vec<InferType>,
    },
}

/// A constraint that still needs a dictionary. Its solution is filled in by
/// `solve_wanted_constraints` before finalization.
#[derive(Clone, Debug)]
pub(super) struct WantedConstraint {
    /// Stable identity, so a nested instance context can refer to this wanted
    /// after the root worklist has moved and generalized it.
    pub(super) id: u32,
    pub(super) class_id: hir::TypeId,
    pub(super) arguments: Vec<InferType>,
    pub(super) dictionary_type: InferType,
    pub(super) span: TextRange,
    /// The enclosing value or instance declaration where a report belongs.
    /// This remains distinct from `span`, the actual constraint's source range.
    pub(super) report_span: TextRange,
    pub(super) givens: Vec<(ClassConstraint, WantedSolution)>,
    pub(super) solution: Option<WantedSolution>,
}

/// A resolved type synonym, expanded during signature elaboration.
#[derive(Clone, Debug)]
pub(super) struct Synonym {
    pub(super) parameters: Vec<String>,
    pub(super) body: hir::Type,
}

/// A data or newtype constructor registered as a value, with its declared
/// result type and field types.
#[derive(Clone, Debug)]
pub(super) struct ConstructorInfo {
    pub(super) symbol: SymbolId,
    pub(super) name: String,
    pub(super) type_id: hir::TypeId,
    pub(super) tag: u32,
    pub(super) parameters: Vec<String>,
    pub(super) fields: Vec<hir::Type>,
}

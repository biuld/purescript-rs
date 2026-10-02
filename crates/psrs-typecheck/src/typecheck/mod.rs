use psrs_hir::{
    self as hir, ExternalKind, Intrinsic, LocalBinder, LocalId, SymbolId, TypeVariableId,
};
use psrs_kind::{CheckedKindEnv, Kind};
use psrs_span::TextRange;
use psrs_thir::{self as thir, Type, TypeId};
use std::collections::{HashMap, HashSet};

mod checked_exports;
mod error;
pub use error::{TypeCheckError, TypeCheckErrorKind};

/// Program-wide semantic inputs needed when checking a module.
#[derive(Clone, Copy)]
pub struct TypecheckContext<'a> {
    pub known_types: &'a [hir::TypeDeclaration],
    pub imported_instances: &'a [hir::InstanceDeclaration],
    pub module_names: &'a HashMap<hir::ModuleId, String>,
    pub checked_kinds: &'a CheckedKindEnv,
}

mod entry;

pub use entry::{
    typecheck_module, typecheck_module_with_checked_kinds,
    typecheck_module_with_checked_kinds_and_module_names, typecheck_module_with_imports,
    typecheck_module_with_imports_and_effect_context,
    typecheck_module_with_imports_and_effect_representation,
};

/// The level of the empty top-level environment. All declaration variables are
/// created at a higher level, so top-level generalization quantifies them.
const TOP_LEVEL: u32 = 0;

/// A type with a set of universally quantified variables and the class
/// constraints those variables must satisfy.
#[derive(Clone, Debug)]
struct Scheme {
    variables: Vec<u32>,
    constraints: Vec<ClassConstraint>,
    ty: InferType,
}

impl Scheme {
    fn monomorphic(ty: InferType) -> Self {
        Self {
            variables: Vec::new(),
            constraints: Vec::new(),
            ty,
        }
    }
}

/// A class constraint `C τ...` recorded during inference. Its `arguments` are
/// the instantiated class type arguments in declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ClassConstraint {
    class_id: hir::TypeId,
    arguments: Vec<InferType>,
    span: TextRange,
}

/// A class method declaration recorded in the class environment.
#[derive(Clone, Debug)]
struct MethodInfo {
    symbol: SymbolId,
    name: String,
    signature: hir::Type,
}

/// One superclass edge of a class. Its arguments are the subclass parameter
/// names that supply the superclass's arguments, in the superclass's parameter
/// order. `field` is the dictionary record field that stores the superclass
/// dictionary.
#[derive(Clone, Debug)]
struct SuperclassInfo {
    class_id: hir::TypeId,
    arguments: Vec<String>,
    field: String,
    span: TextRange,
}

/// One functional dependency of a class, resolved to parameter positions. The
/// `determining` parameters functionally determine the `determined` ones.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FundepInfo {
    determining: Vec<usize>,
    determined: Vec<usize>,
}

/// A class with its ordered type parameters, superclass edges, methods, and
/// functional dependencies.
#[derive(Clone, Debug)]
struct ClassInfo {
    parameters: Vec<String>,
    superclasses: Vec<SuperclassInfo>,
    fundeps: Vec<FundepInfo>,
    methods: Vec<MethodInfo>,
}

/// An instance's class, head arguments (which may contain instance variables),
/// elaborated context constraints, and the synthesized dictionary parameters
/// for that context.
#[derive(Clone, Debug)]
struct InstanceInfo {
    symbol: SymbolId,
    class_id: hir::TypeId,
    chain_id: u32,
    chain_position: u32,
    head_arguments: Vec<InferType>,
    head_variables: HashMap<String, InferType>,
    context: Vec<ClassConstraint>,
    context_parameters: Vec<(LocalId, InferType)>,
}

/// The dictionary selected for a wanted constraint during solving. A
/// superclass selection embeds the parent constraint's already-solved
/// dictionary, and an instance selection embeds the solved context constraints
/// whose dictionaries it applies the constructor to.
#[derive(Clone, Debug)]
enum WantedSolution {
    Given(LocalId),
    Global(SymbolId),
    Instance {
        constructor: SymbolId,
        constructor_type: InferType,
        context: Vec<WantedConstraint>,
    },
    Superclass {
        parent: Box<WantedConstraint>,
        field: String,
    },
    Coercible {
        source: InferType,
        target: InferType,
    },
}

/// A constraint that still needs a dictionary. Its solution is filled in by
/// `solve_wanted_constraints` before finalization.
#[derive(Clone, Debug)]
struct WantedConstraint {
    class_id: hir::TypeId,
    arguments: Vec<InferType>,
    dictionary_type: InferType,
    span: TextRange,
    givens: Vec<(ClassConstraint, WantedSolution)>,
    solution: Option<WantedSolution>,
}

#[derive(Clone, Debug)]
struct InferredDeclaration {
    symbol: SymbolId,
    name: String,
    name_span: TextRange,
    scheme: Scheme,
    value: InferredExpr,
    span: TextRange,
}

#[derive(Clone, Debug)]
struct InferredBinder {
    binder: LocalBinder,
    scheme: Scheme,
}

#[derive(Clone, Debug)]
struct InferredBinding {
    binder: InferredBinder,
    value: InferredExpr,
    span: TextRange,
}

#[derive(Clone, Debug)]
struct InferredExpr {
    kind: InferredExprKind,
    ty: InferType,
    span: TextRange,
}

#[derive(Clone, Debug)]
enum InferredExprKind {
    Local(LocalId),
    Global(SymbolId),
    Integer(i32),
    Number(String),
    Boolean(bool),
    String(String),
    Char(char),
    Array(Vec<InferredExpr>),
    Record(Vec<(String, InferredExpr)>),
    RecordUpdate {
        expression: Box<InferredExpr>,
        fields: Vec<(String, InferredExpr)>,
    },
    FieldAccess {
        expression: Box<InferredExpr>,
        field: String,
    },
    /// A class method selected from the dictionary solved for `wanted`.
    Method {
        method: String,
        wanted: usize,
    },
    /// A constrained function applied to the dictionary solved for `wanted`.
    DictionaryApplication {
        function: Box<InferredExpr>,
        wanted: usize,
    },
    /// The `Safe.Coerce.coerce` function. Its type relation is checked before
    /// finalization, which closes this into a typed representation cast.
    CoerceFunction {
        wanted: usize,
        source: InferType,
        target: InferType,
    },
    /// A dictionary solved for `wanted`, used directly (for example as an
    /// instance's superclass field).
    Evidence(usize),
    Application(Box<InferredExpr>, Box<InferredExpr>),
    Lambda {
        binder: InferredBinder,
        body: Box<InferredExpr>,
    },
    Let {
        bindings: Vec<InferredBinding>,
        body: Box<InferredExpr>,
    },
    If {
        condition: Box<InferredExpr>,
        then_branch: Box<InferredExpr>,
        else_branch: Box<InferredExpr>,
    },
    Case {
        scrutinee: Box<InferredExpr>,
        branches: Vec<InferredCaseBranch>,
    },
}

#[derive(Clone, Debug)]
struct InferredCaseBranch {
    pattern: InferredPattern,
    value: InferredExpr,
    span: TextRange,
    coverage: hir::CaseBranchCoverage,
}

#[derive(Clone, Debug)]
struct InferredPattern {
    kind: InferredPatternKind,
    ty: InferType,
    span: TextRange,
}

#[derive(Clone, Debug)]
enum InferredPatternKind {
    Wildcard,
    Literal {
        literal: thir::PatternLiteral,
    },
    Array {
        elements: Vec<InferredPattern>,
    },
    Named {
        binder: LocalBinder,
        pattern: Box<InferredPattern>,
    },
    Var {
        binder: LocalBinder,
        ty: InferType,
    },
    Constructor {
        symbol: SymbolId,
        arguments: Vec<InferredPattern>,
    },
    Record {
        fields: Vec<(String, InferredPattern)>,
    },
}

/// A resolved type synonym, expanded during signature elaboration.
#[derive(Clone, Debug)]
struct Synonym {
    parameters: Vec<String>,
    body: hir::Type,
}

/// A data or newtype constructor registered as a value, with its declared
/// result type and field types.
#[derive(Clone, Debug)]
struct ConstructorInfo {
    symbol: SymbolId,
    name: String,
    type_id: hir::TypeId,
    tag: u32,
    parameters: Vec<String>,
    fields: Vec<hir::Type>,
}

struct Checker {
    /// Identity of the module currently being checked. It scopes instance
    /// visibility: a local instance is always a candidate, and an imported
    /// instance is a candidate only when its defining module is the class's
    /// module or a module of a type constructor in the wanted arguments.
    module_id: hir::ModuleId,
    globals: HashMap<SymbolId, Scheme>,
    external_kinds: HashMap<SymbolId, ExternalKind>,
    external_signatures: HashMap<SymbolId, hir::Type>,
    /// Declared types of values imported from other modules, keyed by the
    /// exporting declaration's symbol.
    imported: HashMap<SymbolId, hir::Type>,
    locals: HashMap<LocalId, Scheme>,
    type_names: HashMap<hir::TypeId, String>,
    type_modules: HashMap<hir::TypeId, String>,
    type_declarations: HashMap<hir::TypeId, hir::TypeDeclaration>,
    visible_newtypes: HashSet<hir::TypeId>,
    synonyms: HashMap<hir::TypeId, Synonym>,
    checked_kinds: CheckedKindEnv,
    /// Kind assigned to each inference type variable. These variables are
    /// shared with the role-aware Coercible solver.
    infer_variable_kinds: HashMap<u32, Kind>,
    next_kind_variable: u32,
    constructor_info: HashMap<SymbolId, ConstructorInfo>,
    expanding: HashSet<hir::TypeId>,
    substitutions: HashMap<u32, InferType>,
    levels: HashMap<u32, u32>,
    generic_variables: HashSet<u32>,
    rigid: HashSet<u32>,
    next_variable: u32,
    level: u32,
    classes: HashMap<hir::TypeId, ClassInfo>,
    class_methods: HashMap<SymbolId, (hir::TypeId, MethodInfo)>,
    instances: Vec<InstanceInfo>,
    /// Dictionary parameters synthesized for the declaration being checked.
    pending_signatures: HashMap<SymbolId, Vec<(LocalId, InferType)>>,
    /// The constraints a constrained declaration may discharge from its
    /// dictionary parameters while checking its body.
    givens: Vec<(ClassConstraint, WantedSolution)>,
    /// Variables made rigid while checking a local given context. Signature
    /// variables are already rigid; instance-head variables enter here.
    given_rigid: Vec<u32>,
    wanted: Vec<WantedConstraint>,
    next_dictionary_local: u32,
    /// Functional-dependency conflicts already reported, keyed by span and
    /// message, so the fixed-point improvement pass does not duplicate them.
    reported_fundep_conflicts: HashSet<(TextRange, String)>,
    /// Source names for type variables in the signature scope currently being
    /// checked. Typed patterns and expression ascriptions reuse these exact
    /// variables instead of elaborating a second rigid variable by name.
    annotation_variables: HashMap<String, InferType>,
    /// Source names attached to quantified variables so entering a nested
    /// forall can extend the annotation scope at the matching expression.
    type_variable_names: HashMap<u32, String>,
    errors: Vec<TypeCheckError>,
}

#[derive(Default)]
struct TypeInterner {
    values: Vec<Type>,
    ids: HashMap<Type, TypeId>,
}

impl TypeInterner {
    fn intern(&mut self, ty: Type) -> TypeId {
        if let Some(id) = self.ids.get(&ty) {
            return *id;
        }
        let id = TypeId(self.values.len() as u32);
        self.values.push(ty.clone());
        self.ids.insert(ty, id);
        id
    }
}

fn occurs(variable: u32, ty: &InferType) -> bool {
    match ty {
        InferType::Variable(other) => variable == *other,
        InferType::Application(function, argument) => {
            occurs(variable, function) || occurs(variable, argument)
        }
        InferType::RowExtend { ty, tail, .. } => occurs(variable, ty) || occurs(variable, tail),
        InferType::ForAll { variables, body } => {
            !variables.contains(&variable) && occurs(variable, body)
        }
        InferType::Constrained { constraints, body } => {
            constraints
                .iter()
                .flat_map(|constraint| &constraint.arguments)
                .any(|argument| occurs(variable, argument))
                || occurs(variable, body)
        }
        InferType::RowEmpty => false,
        InferType::Constructor(_) => false,
        // A type-level literal contains no variable, so it can never be the
        // type a variable is solved to recursively.
        InferType::TypeLevelString(_) | InferType::TypeLevelInt(_) => false,
    }
}

#[cfg(test)]
mod tests;

mod classes;
mod finalize;
mod generalize;
mod infer;
mod order;
mod rank_n;
mod rows;
mod signature;
mod type_model;
mod unify;

use type_model::*;

use psrs_hir::{
    self as hir, ExternalKind, Intrinsic, LocalBinder, LocalId, SymbolId, TypeVariableId,
};
use psrs_span::TextRange;
use psrs_thir::{self as thir, Type, TypeId};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeCheckErrorKind {
    InvalidHir,
    TypeMismatch,
    OccursCheck,
    UnconstrainedType,
    IntegerOutOfRange,
    NumberOutOfRange,
    UnsupportedExpression,
    UnsupportedType,
    UnsupportedIntrinsic,
    UnloweredOperator,
    UnsupportedClass,
    NoInstance,
    MissingInstanceMethod,
    /// A functional dependency's determined positions disagree, so no single
    /// type can satisfy the constraint.
    FundepConflict,
    /// A constraint still mentions variables that neither the result type nor
    /// the class's functional dependencies determine.
    AmbiguousConstraint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeCheckError {
    pub kind: TypeCheckErrorKind,
    pub span: TextRange,
    message: String,
}

impl TypeCheckError {
    fn new(kind: TypeCheckErrorKind, span: TextRange, message: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

mod entry;

pub use entry::{
    typecheck_module, typecheck_module_with_imports,
    typecheck_module_with_imports_and_effect_context,
    typecheck_module_with_imports_and_effect_representation,
};

/// The level of the empty top-level environment. All declaration variables are
/// created at a higher level, so top-level generalization quantifies them.
const TOP_LEVEL: u32 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
enum InferType {
    Variable(u32),
    Constructor(TypeConstructor),
    Application(Box<InferType>, Box<InferType>),
    /// The empty row. A closed record's row ends here.
    RowEmpty,
    /// A row extended with one labeled field. A record type is
    /// `Application(Constructor(Record), row)`.
    RowExtend {
        label: String,
        ty: Box<InferType>,
        tail: Box<InferType>,
    },
}

/// The arrow `parameter -> result` as the application spine
/// `Application(Application(Constructor(Function), parameter), result)`.
fn arrow(parameter: InferType, result: InferType) -> InferType {
    InferType::Application(
        Box::new(InferType::Application(
            Box::new(InferType::Constructor(TypeConstructor::Function)),
            Box::new(parameter),
        )),
        Box::new(result),
    )
}

/// The parameter and result of an arrow spine `Application(Application(
/// Constructor(Function), parameter), result)`.
fn infer_arrow_parts(function: &InferType, result: &InferType) -> Option<(InferType, InferType)> {
    let InferType::Application(head, parameter) = function else {
        return None;
    };
    matches!(**head, InferType::Constructor(TypeConstructor::Function))
        .then(|| ((**parameter).clone(), result.clone()))
}

/// Builds the row `RowExtend` chain over `fields` in canonical (label-sorted)
/// order, ending in `tail`.
fn row_from_fields(mut fields: Vec<(String, InferType)>, tail: InferType) -> InferType {
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    let mut row = tail;
    for (label, ty) in fields.into_iter().rev() {
        row = InferType::RowExtend {
            label,
            ty: Box::new(ty),
            tail: Box::new(row),
        };
    }
    row
}

/// A record type `Application(Constructor(Record), row)` over `fields` in
/// canonical order, ending in `tail` (`RowEmpty` when closed).
fn record_type(fields: Vec<(String, InferType)>, tail: InferType) -> InferType {
    InferType::Application(
        Box::new(InferType::Constructor(TypeConstructor::Record)),
        Box::new(row_from_fields(fields, tail)),
    )
}

/// The row of a record type, or `None` when `ty` is not a record.
fn record_row(ty: &InferType) -> Option<InferType> {
    let InferType::Application(function, row) = ty else {
        return None;
    };
    matches!(**function, InferType::Constructor(TypeConstructor::Record)).then(|| (**row).clone())
}

/// A row flattened into its fields and its tail. `Closed` is the empty tail;
/// `Open` is a row variable, rigid when it comes from a signature and flexible
/// when it is inferred. Field order is not significant.
struct FlatRow {
    fields: Vec<(String, InferType)>,
    tail: RowTail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowTail {
    Closed,
    Open(u32),
}

impl RowTail {
    fn to_type(self) -> InferType {
        match self {
            RowTail::Closed => InferType::RowEmpty,
            RowTail::Open(variable) => InferType::Variable(variable),
        }
    }
}

/// A type constructor during inference. The arrow, record, and scalar
/// primitives share this head; `Effect` is the trusted effect constructor; user
/// constructors keep their resolved HIR ID so distinct declarations never unify
/// by accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TypeConstructor {
    Function,
    Record,
    Array,
    Effect,
    Int,
    Number,
    Boolean,
    String,
    Char,
    Unit,
    User(hir::TypeId),
}

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
#[derive(Clone, Debug)]
struct ClassConstraint {
    class_id: hir::TypeId,
    arguments: Vec<InferType>,
    span: TextRange,
}

/// A class method declaration recorded in the class environment.
#[derive(Clone, Debug)]
struct MethodInfo {
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
    head_arguments: Vec<InferType>,
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
}

/// A constraint that still needs a dictionary. Its solution is filled in by
/// `solve_wanted_constraints` before finalization.
#[derive(Clone, Debug)]
struct WantedConstraint {
    class_id: hir::TypeId,
    arguments: Vec<InferType>,
    dictionary_type: InferType,
    span: TextRange,
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
    globals: HashMap<SymbolId, Scheme>,
    external_kinds: HashMap<SymbolId, ExternalKind>,
    external_signatures: HashMap<SymbolId, hir::Type>,
    /// Declared types of values imported from other modules, keyed by the
    /// exporting declaration's symbol.
    imported: HashMap<SymbolId, hir::Type>,
    locals: HashMap<LocalId, Scheme>,
    type_names: HashMap<hir::TypeId, String>,
    synonyms: HashMap<hir::TypeId, Synonym>,
    effect_type: Option<hir::TypeId>,
    effect_runtime_representation: bool,
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
    wanted: Vec<WantedConstraint>,
    next_dictionary_local: u32,
    /// Functional-dependency conflicts already reported, keyed by span and
    /// message, so the fixed-point improvement pass does not duplicate them.
    reported_fundep_conflicts: HashSet<(TextRange, String)>,
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
        InferType::RowEmpty => false,
        InferType::Constructor(_) => false,
    }
}

#[cfg(test)]
mod tests;

mod classes;
mod finalize;
mod infer;
mod order;
mod rows;
mod signature;
mod unify;

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
    I32,
    F64,
    Boolean,
    String,
    Char,
    Unit,
    Constructor(TypeConstructor),
    Application(Box<InferType>, Box<InferType>),
    Record(InferRecord),
    Function(Box<InferType>, Box<InferType>),
}

/// A record row during inference. `Closed` is the empty tail. `Open` is a row
/// variable, rigid when it comes from a signature and flexible when it is
/// inferred. Field order is not significant; labels are kept sorted.
#[derive(Clone, Debug, PartialEq, Eq)]
struct InferRecord {
    fields: Vec<(String, InferType)>,
    tail: RowTail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowTail {
    Closed,
    Open(u32),
}

impl InferRecord {
    fn closed(mut fields: Vec<(String, InferType)>) -> Self {
        fields.sort_by(|left, right| left.0.cmp(&right.0));
        Self {
            fields,
            tail: RowTail::Closed,
        }
    }
}

/// A type constructor during inference. `Array` is the only built-in the
/// current front end elaborates; user constructors keep their resolved HIR ID so
/// distinct declarations never unify by accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TypeConstructor {
    Array,
    Effect,
    User(hir::TypeId),
}

/// A type with a set of universally quantified variables.
#[derive(Clone, Debug)]
struct Scheme {
    variables: Vec<u32>,
    ty: InferType,
}

impl Scheme {
    fn monomorphic(ty: InferType) -> Self {
        Self {
            variables: Vec::new(),
            ty,
        }
    }
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
        InferType::Application(function, argument) | InferType::Function(function, argument) => {
            occurs(variable, function) || occurs(variable, argument)
        }
        InferType::Record(record) => {
            record
                .fields
                .iter()
                .any(|(_, field)| occurs(variable, field))
                || matches!(record.tail, RowTail::Open(tail) if tail == variable)
        }
        InferType::I32
        | InferType::F64
        | InferType::Boolean
        | InferType::String
        | InferType::Char
        | InferType::Unit
        | InferType::Constructor(_) => false,
    }
}

#[cfg(test)]
mod tests;

mod finalize;
mod infer;
mod order;
mod rows;
mod signature;
mod unify;

use super::TypeId;
use psrs_span::TextRange;

/// A resolved type signature. Type names have been resolved to built-in
/// constructors or type variables; the source span is retained for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: TextRange,
}

/// A resolved type variable binder. Kind annotations are only meaningful on
/// `forall` and type-parameter binders; elsewhere `kind` is `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeParameter {
    pub name: String,
    pub name_span: TextRange,
    pub kind: Option<Type>,
}

/// A resolved row field used by row and record types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeField {
    pub label: String,
    pub label_span: TextRange,
    pub ty: Type,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeKind {
    Variable(String),
    Constructor(BuiltinType),
    /// A user-defined type constructor, synonym, or class identified by ID.
    Named(TypeId),
    Application(Box<Type>, Box<Type>),
    Function {
        parameter: Box<Type>,
        result: Box<Type>,
    },
    Forall {
        variables: Vec<TypeParameter>,
        body: Box<Type>,
    },
    Constrained {
        constraint: Box<Type>,
        body: Box<Type>,
    },
    Row {
        fields: Vec<TypeField>,
        tail: Option<Box<Type>>,
    },
    Record {
        fields: Vec<TypeField>,
        tail: Option<Box<Type>>,
    },
    /// A type-level integer literal, whose kind is `Int`.
    Integer(String),
    /// A type-level string literal, whose kind is `Symbol`.
    String(String),
}

/// A built-in type or kind constructor known to the compiler. `Type`,
/// `Constraint`, `Symbol`, `Row`, `Record`, and `Function` name `Prim`
/// entities and double as kind constructors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinType {
    Int,
    Number,
    Boolean,
    String,
    Char,
    Unit,
    Type,
    Constraint,
    Symbol,
    Function,
    Row,
    Record,
    Array,
    /// The platform-independent effect constructor. The type checker
    /// elaborates `Effect a` to the function type used by the bootstrap
    /// runtime until a dedicated effect runtime is available.
    Effect,
}

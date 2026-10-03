use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InferType {
    Variable(u32),
    Constructor(TypeConstructor),
    Application(Box<InferType>, Box<InferType>),
    /// Universally quantified type variables scoped over `body`. These are
    /// structural binders, distinct from variables quantified by a value scheme.
    ForAll {
        variables: Vec<u32>,
        body: Box<InferType>,
    },
    /// A qualified type whose dictionary requirements are in scope only for
    /// the qualified body. Finalization turns each requirement into an
    /// explicit dictionary argument in THIR.
    Constrained {
        constraints: Vec<ClassConstraint>,
        body: Box<InferType>,
    },
    RowEmpty,
    RowExtend {
        label: String,
        ty: Box<InferType>,
        tail: Box<InferType>,
    },
    /// A type-level string literal, of kind `Symbol`. The payload is a sequence
    /// of Unicode scalar values (DEC-16), so an unpaired surrogate cannot be
    /// represented and never reaches the solver. Two literals are equal when
    /// their scalar sequences are equal.
    TypeLevelString(String),
    /// A type-level integer literal, of kind `Int`. Two literals are equal when
    /// their values are equal. A literal is decided: it never solves for a
    /// variable, it is what a variable is solved to.
    TypeLevelInt(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum TypeConstructor {
    Function,
    Record,
    /// The `Prim.Row` type constructor of kind `Type -> Type`. Official PureScript
    /// declares it with a phantom role, so it is a nominal head whose
    /// application is not itself a row: a row value is [`InferType::RowEmpty`],
    /// [`InferType::RowExtend`], or a variable whose recorded kind is `Row k`.
    Row,
    Array,
    Int,
    Number,
    Boolean,
    String,
    Char,
    Unit,
    /// The `Prim.Type` kind constructor. Official PureScript declares it with
    /// kind `Type`, so naming it in a type position is a nominal type on the
    /// same spine as every other head, not a kind read in a type position.
    Type,
    /// The `Prim.Constraint` kind constructor, of kind `Type`.
    Constraint,
    /// The `Prim.Symbol` kind constructor. It is the kind of a type-level string
    /// and, like `Type` and `Constraint`, a type of kind `Type`.
    Symbol,
    User(hir::TypeId),
}

pub(super) fn arrow(parameter: InferType, result: InferType) -> InferType {
    InferType::Application(
        Box::new(InferType::Application(
            Box::new(InferType::Constructor(TypeConstructor::Function)),
            Box::new(parameter),
        )),
        Box::new(result),
    )
}

pub(super) fn infer_arrow_parts(
    function: &InferType,
    result: &InferType,
) -> Option<(InferType, InferType)> {
    let InferType::Application(head, parameter) = function else {
        return None;
    };
    matches!(**head, InferType::Constructor(TypeConstructor::Function))
        .then(|| ((**parameter).clone(), result.clone()))
}

pub(super) fn row_from_fields(mut fields: Vec<(String, InferType)>, tail: InferType) -> InferType {
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

pub(super) fn record_type(fields: Vec<(String, InferType)>, tail: InferType) -> InferType {
    InferType::Application(
        Box::new(InferType::Constructor(TypeConstructor::Record)),
        Box::new(row_from_fields(fields, tail)),
    )
}

pub(super) fn record_row(ty: &InferType) -> Option<InferType> {
    let InferType::Application(function, row) = ty else {
        return None;
    };
    matches!(**function, InferType::Constructor(TypeConstructor::Record)).then(|| (**row).clone())
}

pub(super) struct FlatRow {
    pub(super) fields: Vec<(String, InferType)>,
    pub(super) tail: RowTail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowTail {
    Closed,
    Open(u32),
}

impl RowTail {
    pub(super) fn to_type(self) -> InferType {
        match self {
            RowTail::Closed => InferType::RowEmpty,
            RowTail::Open(variable) => InferType::Variable(variable),
        }
    }
}

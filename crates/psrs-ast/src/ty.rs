use crate::{Name, TypeParameter};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: TextRange,
}

/// A row or record field. The label is unresolved; resolution replaces it with
/// the same spelling so row checks can compare labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeField {
    pub label: Name,
    pub ty: Type,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeKind {
    Name(Name),
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
    Integer(String),
    String(String),
}

use super::{SymbolId, Type, TypeId, TypeParameter};
use psrs_span::TextRange;

/// The declaration form a named type-level entity comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeDeclarationKind {
    Data,
    Newtype,
    TypeSynonym,
    Class,
}

/// A resolved type-level declaration. Its `id` names the type, class, or
/// synonym; data and newtype constructors also introduce value symbols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDeclaration {
    pub id: TypeId,
    pub name: String,
    pub name_span: TextRange,
    pub kind: TypeDeclarationKind,
    pub parameters: Vec<TypeParameter>,
    pub constructors: Vec<Constructor>,
    pub members: Vec<ClassMember>,
    /// The body of a type synonym, when `kind` is `TypeSynonym`.
    pub body: Option<Type>,
    /// The superclass constraints of a class, when `kind` is `Class`.
    pub superclasses: Vec<Type>,
    /// The kind declared by a standalone `data T :: K` signature, when present.
    pub declared_kind: Option<Type>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constructor {
    /// The symbol the constructor has in the value namespace.
    pub symbol: SymbolId,
    pub name: String,
    pub name_span: TextRange,
    pub fields: Vec<Type>,
    pub span: TextRange,
}

/// A class method, which is a value provided by each instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassMember {
    pub symbol: SymbolId,
    pub name: String,
    pub name_span: TextRange,
    pub signature: Option<Type>,
    pub span: TextRange,
}

use super::{Expr, SymbolId, Type, TypeId, TypeParameter};
use psrs_span::TextRange;

/// The declaration form a named type-level entity comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeDeclarationKind {
    Data,
    Newtype,
    TypeSynonym,
    Class,
    /// `foreign import data`. Nominal, with a declared kind and no constructors.
    Foreign,
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
    /// The functional dependencies of a class, when `kind` is `Class`. Each
    /// side names type parameters; resolution to parameter positions happens
    /// when the class environment is built.
    pub fundeps: Vec<FunctionalDependency>,
    /// The kind declared by a standalone `data T :: K` signature, when present.
    pub declared_kind: Option<Type>,
    pub span: TextRange,
}

/// A resolved `class ... | from -> to` functional dependency. Both sides name
/// one or more of the class's type parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionalDependency {
    pub from: Vec<String>,
    pub to: Vec<String>,
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

/// A resolved `instance` declaration. Its `symbol` names the dictionary value
/// the instance elaborates to; `head` is the class applied to the instance's
/// type arguments and `context` lists the instance's context constraints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceDeclaration {
    pub symbol: SymbolId,
    pub name: String,
    pub name_span: TextRange,
    pub class_id: TypeId,
    pub context: Vec<Type>,
    pub head: Type,
    pub members: Vec<InstanceMember>,
    pub span: TextRange,
}

/// One method implementation supplied by an instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceMember {
    pub name: String,
    pub name_span: TextRange,
    pub value: Expr,
    pub span: TextRange,
}

use super::{Expr, SymbolId, Type, TypeId, TypeParameter};
use psrs_span::TextRange;

/// The declaration form a named type-level entity comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeDeclarationKind {
    Data,
    Newtype,
    TypeSynonym,
    Class,
    /// `foreign import data`, with a declared kind and no constructors. Its
    /// role vector is nominal unless an explicit role signature is supplied.
    Foreign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// The most restrictive role: arguments must be equal.
    Nominal,
    Representational,
    Phantom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleDeclaration {
    pub roles: Vec<(Role, TextRange)>,
    pub span: TextRange,
}

/// A resolved type-level declaration. Its `id` names the type, class, or
/// synonym; data and newtype constructors also introduce value symbols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDeclaration {
    pub id: TypeId,
    pub name: String,
    pub name_span: TextRange,
    pub kind: TypeDeclarationKind,
    /// A canonical library interface identity attached by resolution. P5
    /// validates the declared dictionary contract before enabling its rule.
    pub compiler_class: Option<super::CompilerClass>,
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
    /// A source role annotation attached to this local data, newtype, or
    /// foreign-data type.
    pub declared_roles: Option<RoleDeclaration>,
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
    /// Module-local ordered instance-chain identity retained across imports.
    pub chain_id: u32,
    /// Zero-based position within `chain_id`.
    pub chain_position: u32,
    pub context: Vec<Type>,
    pub head: Type,
    pub members: Vec<InstanceMember>,
    /// Compiler derivation strategy attached to a source `derive instance`.
    /// The resolved typechecker owns method synthesis from this marker.
    pub derivation: Option<DerivationStrategy>,
    pub span: TextRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DerivationStrategy {
    KnownClass,
    Newtype,
}

/// One method implementation supplied by an instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceMember {
    pub name: String,
    pub name_span: TextRange,
    pub value: Expr,
    pub span: TextRange,
}

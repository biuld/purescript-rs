use super::{SymbolId, TypeReference};
use psrs_span::TextRange;

/// A value introduced into a module's scope by an import declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedSymbol {
    /// The symbol as declared in the imported module.
    pub symbol: SymbolId,
    /// The name used to reference the symbol inside the importing module.
    pub local_name: String,
    /// The name the symbol has in its declaring module.
    pub external_name: String,
    pub span: TextRange,
}

/// A type, class, or synonym introduced into a module's scope by an import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedType {
    /// The compiler primitive or declaration as exposed by the imported module.
    pub reference: TypeReference,
    /// The name used to reference the type inside the importing module.
    pub name: String,
    pub span: TextRange,
    /// Whether the imported type is an opaque `foreign import data` declaration.
    /// Importers must keep that nominal identity; the declaring module is not
    /// consulted again when a signature mentions the type.
    pub opaque: bool,
}

/// A resolved import declaration. The imported module is identified by ID and
/// the names it contributes are enumerated so name resolution and the HIR
/// verifier do not need the module graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub module: super::ModuleId,
    pub module_name: String,
    pub alias: Option<String>,
    pub hiding: bool,
    pub symbols: Vec<ImportedSymbol>,
    pub types: Vec<ImportedType>,
    pub fixities: Vec<Fixity>,
    pub span: TextRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixityNamespace {
    Value,
    Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Associativity {
    Left,
    Right,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixityTarget {
    Value(SymbolId),
    Type(TypeReference),
}

/// A fixity alias resolved to the declaration it names. Operator spellings
/// remain separate from declaration names because several spellings can refer
/// to one value or type with different fixities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fixity {
    pub namespace: FixityNamespace,
    pub operator: String,
    pub target_name: String,
    pub target: FixityTarget,
    pub associativity: Associativity,
    pub precedence: u32,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedOperator {
    pub symbol: SymbolId,
    pub name: String,
    pub target_name: String,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedTypeOperator {
    pub reference: TypeReference,
    pub name: String,
    pub target_name: String,
    pub span: TextRange,
}

/// A value re-exported by a module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedSymbol {
    pub symbol: SymbolId,
    pub name: String,
    pub span: TextRange,
}

/// A type, synonym, or class re-exported by a module. `constructors` is `None`
/// when the type is exported without any of its data constructors; `Some` lists
/// exactly the exported constructors, which may be empty for `T()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedType {
    pub reference: TypeReference,
    pub name: String,
    pub name_span: TextRange,
    pub constructors: Option<Vec<SymbolId>>,
    pub is_class: bool,
    /// Whether this export is an opaque `foreign import data` type. Re-exports
    /// keep the flag so a later importer still treats the type as opaque.
    pub opaque: bool,
}

/// An instance made visible by the module's implicit instance export rule.
/// Instances are not selected by import lists; a consumer imports all visible
/// instances from each module in its dependency closure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportedInstance {
    pub symbol: SymbolId,
    pub name: String,
    pub name_span: TextRange,
}

/// A resolved explicit export list. The absence of a list means the module
/// exports every declaration it defines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportList {
    pub values: Vec<ExportedSymbol>,
    pub operators: Vec<ExportedOperator>,
    pub types: Vec<ExportedType>,
    pub type_operators: Vec<ExportedTypeOperator>,
    pub instances: Vec<ExportedInstance>,
    pub span: TextRange,
}

use super::{SymbolId, TypeId};
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
    /// The `TypeId` as declared in the imported module.
    pub id: TypeId,
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
    pub id: TypeId,
    pub name: String,
    pub name_span: TextRange,
    pub constructors: Option<Vec<SymbolId>>,
    pub is_class: bool,
    /// Whether this export is an opaque `foreign import data` type. Re-exports
    /// keep the flag so a later importer still treats the type as opaque.
    pub opaque: bool,
}

/// A resolved explicit export list. The absence of a list means the module
/// exports every declaration it defines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportList {
    pub values: Vec<ExportedSymbol>,
    pub types: Vec<ExportedType>,
    pub span: TextRange,
}

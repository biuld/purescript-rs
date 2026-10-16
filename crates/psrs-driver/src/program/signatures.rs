//! The program-wide signature table and its per-module import view.

use std::collections::HashMap;

/// The declared type of every value and external in the program, keyed by the
/// symbol that declares it. A re-exported symbol keeps the signature of the
/// declaration that owns it, so the table is global rather than per module.
pub(super) fn declared_signatures(
    modules: &[psrs_hir::Module],
) -> HashMap<psrs_hir::SymbolId, psrs_hir::Type> {
    modules
        .iter()
        .flat_map(|module| {
            let declarations = module.declarations.iter().filter_map(|declaration| {
                declaration
                    .signature
                    .clone()
                    .map(|signature| (declaration.symbol, signature))
            });
            // Only a source `foreign import` is declared in a
            // module; a compiler intrinsic is not, even though its descriptor
            // gives it a signature.
            let externals = module.externals.iter().filter_map(|external| {
                if external.kind.requires_checked_signature() {
                    external
                        .signature
                        .clone()
                        .map(|signature| (external.symbol, signature))
                } else {
                    None
                }
            });
            declarations.chain(externals)
        })
        .collect()
}

/// Resolves a module's imported symbols to their declared type from the global
/// declaration table. A symbol re-exported by an umbrella module keeps the
/// signature of the declaration that owns it.
pub(super) fn imported_signatures(
    module: &psrs_hir::Module,
    signatures: &HashMap<psrs_hir::SymbolId, psrs_hir::Type>,
) -> HashMap<psrs_hir::SymbolId, psrs_hir::Type> {
    let mut imported = HashMap::new();
    for import in &module.imports {
        for symbol in &import.symbols {
            if let Some(ty) = signatures.get(&symbol.symbol) {
                imported.insert(symbol.symbol, ty.clone());
            }
        }
    }
    imported
}

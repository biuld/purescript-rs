mod alpha;
mod boolean_case;
mod boolean_product_case;
mod case_helpers;
mod constant_truth;
mod expr;
mod free_vars;
mod guards;

use psrs_hir as hir;

/// Lowers resolved operator and guarded equation nodes into ordinary HIR.
pub fn desugar_module(module: hir::Module) -> Result<hir::Module, Vec<hir::VerifyError>> {
    let true_symbols = constant_truth::true_symbols(std::slice::from_ref(&module));
    desugar_module_with_true_symbols(module, &true_symbols)
}

/// Resolves statically true declarations over a whole resolved program.
pub fn true_symbols(modules: &[hir::Module]) -> std::collections::HashSet<hir::SymbolId> {
    constant_truth::true_symbols(modules)
}

/// Lowers one module using constant-true declarations discovered in the
/// complete resolved program, including imported aliases and re-exports.
pub fn desugar_module_with_true_symbols(
    module: hir::Module,
    true_symbols: &std::collections::HashSet<hir::SymbolId>,
) -> Result<hir::Module, Vec<hir::VerifyError>> {
    module.verify()?;
    let mut desugarer = expr::Desugarer::new(&module, true_symbols);
    let mut module = module;
    for declaration in &mut module.declarations {
        declaration.value = desugarer.lower(declaration.value.clone());
    }
    for instance in &mut module.instances {
        for member in &mut instance.members {
            member.value = desugarer.lower(member.value.clone());
        }
    }
    if !desugarer.errors.is_empty() {
        return Err(desugarer.errors);
    }
    module.verify_normalized()?;
    Ok(module)
}

#[cfg(test)]
mod tests;

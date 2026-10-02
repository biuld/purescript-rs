mod alpha;
mod boolean_case;
mod case_helpers;
mod expr;
mod free_vars;
mod guards;

use psrs_hir as hir;

/// Lowers resolved operator and guarded equation nodes into ordinary HIR.
pub fn desugar_module(module: hir::Module) -> Result<hir::Module, Vec<hir::VerifyError>> {
    module.verify()?;
    let mut desugarer = expr::Desugarer::new(&module);
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

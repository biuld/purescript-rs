//! Representation-preserving P10 optimization over verified MIR.

mod cfg;
mod constants;
mod effects;
mod imports;
mod inline;
mod values;

#[cfg(test)]
mod tests;

use super::{Module, verify_module_with_capabilities};
use crate::{BackendError, TargetCapabilities, annotate_errors};

/// Optimizes verified MIR without changing its concrete types or signatures.
pub fn optimize(
    mut module: Module,
    target: TargetCapabilities,
) -> Result<Module, Vec<BackendError>> {
    verify(&module, target)?;

    if inline::inline_small_functions(&mut module) {
        verify(&module, target)?;
    }

    loop {
        let mut changed = cfg::prune_unreachable(&mut module);
        verify(&module, target)?;

        changed |= constants::propagate(&mut module);
        verify(&module, target)?;

        changed |= cfg::simplify_terminators(&mut module);
        // Removing an untaken successor can leave the other successor
        // unreachable while it still names values from the rewritten block.
        // Drop unreachable blocks before verifying; the verifier intentionally
        // requires dominance over every block it sees.
        changed |= cfg::prune_unreachable(&mut module);
        verify(&module, target)?;

        if !changed {
            break;
        }
    }

    values::forward_copies(&mut module);
    verify(&module, target)?;

    values::eliminate_dead_pure(&mut module);
    verify(&module, target)?;

    imports::project_reachable(&mut module);
    verify(&module, target)?;
    Ok(module)
}

fn verify(module: &Module, target: TargetCapabilities) -> Result<(), Vec<BackendError>> {
    verify_module_with_capabilities(module, target).map_err(|mut errors| {
        for error in &mut errors {
            error.pass = "P10 MIR optimization";
        }
        annotate_errors(errors, module.entry.map(|entry| entry.module))
    })
}

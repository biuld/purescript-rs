//! P7 simplification and specialization of verified Typed Core.

mod dead;
mod effects;
mod inline;
mod simplify;
mod util;

use crate::{Module, VerifyError};

/// Resource limits for deterministic P7 optimization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Maximum complete simplification rounds.
    pub max_iterations: usize,
    /// Maximum Core nodes in a lambda body eligible for beta reduction.
    pub max_inline_nodes: usize,
    /// Maximum beta reductions in a single round.
    pub max_inline_sites: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_iterations: 4,
            max_inline_nodes: 24,
            max_inline_sites: 128,
        }
    }
}

/// Optimizes a linked Typed Core module, checking Core invariants before and
/// after every pass. Unknown calls and potentially trapping expressions remain
/// observable, so their evaluation is never removed by dead-binding cleanup.
pub fn optimize(mut module: Module, budget: Budget) -> Result<Module, Vec<VerifyError>> {
    module.verify()?;
    for _ in 0..budget.max_iterations {
        let before = module.clone();

        module = simplify::run(module);
        module.verify()?;

        module = inline::run(module, budget);
        module.verify()?;

        module = dead::run(module);
        module.verify()?;

        if module == before {
            break;
        }
    }
    Ok(module)
}

#[cfg(test)]
mod tests;

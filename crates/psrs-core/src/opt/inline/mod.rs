mod alpha;
mod global;
mod local;

use super::Budget;
use crate::Module;

pub(super) fn run(mut module: Module, budget: Budget) -> Module {
    if budget.max_inline_nodes == 0 || budget.max_inline_sites == 0 {
        return module;
    }
    let mut sites_left = budget.max_inline_sites;
    module = local::run(module, budget.max_inline_nodes, &mut sites_left);
    if sites_left > 0 {
        module = global::run(module, budget.max_inline_nodes, &mut sites_left);
    }
    module
}

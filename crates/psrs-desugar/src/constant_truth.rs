use psrs_hir::{Expr, ExprKind, ExternalKind, Intrinsic, LocalId, Module, SymbolId};
use std::collections::HashSet;

/// Finds resolved global values whose definitions are transparently Boolean
/// true aliases. Re-exports keep their declaring SymbolId, so import spelling
/// and route do not affect the result.
pub fn true_symbols(modules: &[Module]) -> HashSet<SymbolId> {
    let mut known = modules
        .iter()
        .flat_map(|module| &module.externals)
        .filter_map(|external| {
            matches!(external.kind, ExternalKind::Intrinsic(Intrinsic::BoolTrue))
                .then_some(external.symbol)
        })
        .collect::<HashSet<_>>();
    loop {
        let mut changed = false;
        for declaration in modules.iter().flat_map(|module| &module.declarations) {
            if evaluates_to_true(&declaration.value, &known, &HashSet::new()) {
                changed |= known.insert(declaration.symbol);
            }
        }
        if !changed {
            return known;
        }
    }
}

fn evaluates_to_true(
    expression: &Expr,
    known: &HashSet<SymbolId>,
    locals: &HashSet<LocalId>,
) -> bool {
    match &expression.kind {
        ExprKind::Global(symbol) => known.contains(symbol),
        ExprKind::Local(id) => locals.contains(id),
        ExprKind::Typed { expression, .. } => evaluates_to_true(expression, known, locals),
        ExprKind::Let { bindings, body } => {
            let mut local_true = locals.clone();
            loop {
                let mut changed = false;
                for binding in bindings {
                    if evaluates_to_true(&binding.value, known, &local_true) {
                        changed |= local_true.insert(binding.binder.id);
                    }
                }
                if !changed {
                    break;
                }
            }
            evaluates_to_true(body, known, &local_true)
        }
        _ => false,
    }
}

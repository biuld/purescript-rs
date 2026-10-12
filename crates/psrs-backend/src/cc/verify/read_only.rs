//! Conservative local call summaries; imported and dynamic calls are unproven.
use crate::cc::{AssignmentKind as A, Function};
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

pub(super) fn functions(functions: &HashMap<SymbolId, &Function>) -> HashSet<SymbolId> {
    let mut dependencies = HashMap::<SymbolId, HashSet<SymbolId>>::new();
    for (symbol, function) in functions {
        let mut calls = HashSet::new();
        let mut safe = true;
        let mut pending = vec![function.assignments.as_slice()];
        while let Some(body) = pending.pop() {
            for assignment in body {
                if let A::TagSwitch {
                    cases,
                    default_assignments,
                    ..
                } = &assignment.kind
                {
                    pending.extend(cases.iter().map(|case| case.assignments.as_slice()));
                    pending.push(default_assignments);
                } else if let A::If {
                    then_assignments,
                    else_assignments,
                    ..
                } = &assignment.kind
                {
                    pending.extend([then_assignments.as_slice(), else_assignments.as_slice()]);
                }
                match &assignment.kind {
                    A::DirectCall { function, .. } => {
                        calls.insert(*function);
                    }
                    // String literals allocate guest arrays and are excluded.
                    A::Constant(_)
                    | A::NumberConstant(_)
                    | A::Primitive { .. }
                    | A::Unary { .. }
                    | A::RepresentationCast { .. }
                    | A::RepresentationTest { .. }
                    | A::VariantGet { .. }
                    | A::VariantTag { .. }
                    | A::ProductGet { .. }
                    | A::ArrayGet { .. }
                    | A::ArrayLen { .. }
                    | A::ClosureGetCapture { .. }
                    | A::TagSwitch { .. }
                    | A::If { .. }
                    | A::Unreachable
                    | A::ControlEvidence { .. } => {}
                    A::AggregateConvert { conversion, .. }
                        if crate::cc::value_identity::transparent(&conversion.plan) => {}
                    _ => {
                        safe = false;
                        break;
                    }
                }
            }
        }
        if safe {
            dependencies.insert(*symbol, calls);
        }
    }
    let mut safe = dependencies.keys().copied().collect::<HashSet<_>>();
    loop {
        let previous = safe.clone();
        safe.retain(|symbol| {
            dependencies[symbol]
                .iter()
                .all(|callee| previous.contains(callee))
        });
        if safe == previous {
            return safe;
        }
    }
}

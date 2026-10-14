//! Total local call summaries. Recursive cycles, reads and traps are unproven.
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
                    A::Primitive {
                        op: crate::cc::BinaryOp::IntQuot | crate::cc::BinaryOp::IntRem,
                        ..
                    } => {
                        safe = false;
                        break;
                    }
                    A::Constant(_)
                    | A::NumberConstant(_)
                    | A::Primitive { .. }
                    | A::Unary { .. }
                    | A::RepresentationTest { .. }
                    | A::TagSwitch { .. }
                    | A::If { .. }
                    | A::ControlEvidence { .. } => {}
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
    // Least fixed point excludes recursion: no-write recursive SCCs may diverge.
    let mut safe = HashSet::new();
    loop {
        let previous = safe.len();
        for (symbol, calls) in &dependencies {
            if calls.iter().all(|callee| safe.contains(callee)) {
                safe.insert(*symbol);
            }
        }
        if safe.len() == previous {
            return safe;
        }
    }
}

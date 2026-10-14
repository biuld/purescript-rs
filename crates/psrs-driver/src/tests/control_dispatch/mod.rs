//! Dispatch must cover its machine domain and prove the selected payload case.
use super::control_checkpoints::{SOURCE, binding};
use psrs_backend::cc::{
    AggregateConvert, Assignment, AssignmentKind as A, ControlHelperRole as R, RefShape, ReprId,
    Representation, RepresentationTable, ValueConversion, ValueDecl, ValueId, ValueShape,
};
use psrs_backend::mir::{Instruction as I, Terminator as T};

pub(super) fn required(role: R) -> &'static [i32] {
    match role {
        R::Executor => &[0, 1, 2, 3, 4, 5],
        R::CloneSequence => &[0, 1, 2, 3],
        R::RebuildSequence => &[0, 1, 3],
        _ => panic!("dispatch helper"),
    }
}

/// The nominal representation a helper parameter carries.
pub(super) fn carrier_repr(table: &RepresentationTable, role: R, index: usize) -> ReprId {
    let helper = table
        .control_helpers
        .iter()
        .find(|helper| helper.role() == role)
        .unwrap();
    match table.signature(helper.signature()).unwrap().parameters[index] {
        ValueShape::Reference(reference) => match reference.heap {
            RefShape::Repr(id) => id,
            _ => panic!("carrier parameter is not a representation reference"),
        },
        _ => panic!("carrier parameter is not a reference"),
    }
}

/// Visits every assignment, including nested switch and if bodies.
pub(super) fn visit(
    body: &mut [Assignment],
    mutate: &mut impl FnMut(&mut Assignment) -> bool,
) -> bool {
    for assignment in body {
        if mutate(assignment) {
            return true;
        }
        match &mut assignment.kind {
            A::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    if visit(&mut case.assignments, mutate) {
                        return true;
                    }
                }
                if visit(default_assignments, mutate) {
                    return true;
                }
            }
            A::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                if visit(then_assignments, mutate) || visit(else_assignments, mutate) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

mod cc;
mod mir;

#[test]
fn cc_rejects_every_missing_machine_dispatch_alternative() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    for role in [R::Executor, R::CloneSequence, R::RebuildSequence] {
        let (symbol, _) = binding(&input.cc.representations, role);
        for tag in required(role) {
            let mut broken = input.cc.clone();
            let f = broken
                .functions
                .iter_mut()
                .find(|f| f.symbol == symbol)
                .unwrap();
            let A::TagSwitch { cases, .. } = &mut f.assignments.last_mut().unwrap().kind else {
                panic!("input dispatch");
            };
            cases.retain(|case| case.tag != *tag);
            let errors = psrs_backend::cc::verify_module(&broken).expect_err("missing alternative");
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("every required alternative")),
                "{role:?}/{tag}: {errors:?}"
            );
        }
    }
}

#[test]
fn mir_rejects_every_missing_machine_dispatch_alternative() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let module = crate::compile_main_stages(SOURCE).unwrap().mir;
    for role in [R::Executor, R::CloneSequence, R::RebuildSequence] {
        let (symbol, _) = binding(&input.cc.representations, role);
        for tag in required(role) {
            let mut broken = module.clone();
            let f = broken
                .functions
                .iter_mut()
                .find(|f| f.symbol == symbol)
                .unwrap();
            let switch = f
                .blocks
                .iter_mut()
                .find_map(|b| match &mut b.terminator {
                    Some(T::Switch { cases, .. })
                        if cases.iter().map(|(tag, _)| *tag).collect::<Vec<_>>()
                            == required(role) =>
                    {
                        Some(cases)
                    }
                    _ => None,
                })
                .expect("input dispatch");
            switch.retain(|(case, _)| case != tag);
            let errors =
                psrs_backend::mir::verify_module(&broken).expect_err("missing alternative");
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("every required alternative")),
                "{role:?}/{tag}: {errors:?}"
            );
        }
    }
}

//! Semantic helper IDs cannot replace proofs of their actual extent behavior.
use super::{SOURCE, binding};
use psrs_backend::cc::{Assignment, AssignmentKind as A, ControlHelperRole as R};

fn visit(body: &mut [Assignment], mutation: &mut impl FnMut(&mut A) -> bool) -> bool {
    for assignment in body {
        if mutation(&mut assignment.kind) {
            return true;
        }
        match &mut assignment.kind {
            A::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    if visit(&mut case.assignments, mutation) {
                        return true;
                    }
                }
                if visit(default_assignments, mutation) {
                    return true;
                }
            }
            A::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                if visit(then_assignments, mutation) || visit(else_assignments, mutation) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

#[test]
fn cc_lifecycle_bodies_reject_identity_wrong_guard_write_and_ancestry() {
    for role in [R::Check, R::Close] {
        for mutation in 0..4 {
            let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
            let (symbol, _) = binding(&input.cc.representations, role);
            let function = input
                .cc
                .functions
                .iter_mut()
                .find(|f| f.symbol == symbol)
                .unwrap();
            let parameter = function.parameters[0];
            match mutation {
                0 => {
                    function.assignments.clear();
                    function.result = parameter;
                }
                1 => assert!(visit(&mut function.assignments, &mut |kind| match kind {
                    A::Constant(value) if *value == if role == R::Check { 0 } else { 1 } => {
                        *value = 2;
                        true
                    }
                    _ => false,
                })),
                2 if role == R::Check => {
                    assert!(visit(&mut function.assignments, &mut |kind| match kind {
                        A::TagSwitch { cases, .. } if cases.len() == 1 => {
                            cases[0].tag = 1;
                            true
                        }
                        _ => false,
                    }))
                }
                2 => assert!(visit(&mut function.assignments, &mut |kind| match kind {
                    A::ArraySet {
                        value,
                        destination,
                        representation,
                        ..
                    } => {
                        *kind = A::RepresentationCast {
                            value: *value,
                            destination: *destination,
                            reference: psrs_backend::cc::Reference {
                                nullable: false,
                                heap: psrs_backend::cc::RefShape::Repr(*representation),
                            },
                        };
                        true
                    }
                    _ => false,
                })),
                _ if role == R::Check => {
                    assert!(visit(&mut function.assignments, &mut |kind| match kind {
                        A::DirectCall {
                            function: target,
                            arguments,
                            ..
                        } if *target == symbol => {
                            arguments[0] = parameter;
                            true
                        }
                        _ => false,
                    }))
                }
                _ => assert!(visit(&mut function.assignments, &mut |kind| match kind {
                    A::ArraySet { value, .. } => {
                        *value = parameter;
                        true
                    }
                    _ => false,
                })),
            }
            let errors =
                psrs_backend::cc::verify_module(&input.cc).expect_err("malformed lifecycle body");
            assert!(
                errors.iter().any(|e| e.pass == "P8 CC verification"),
                "{role:?}/{mutation}: {errors:?}"
            );
        }
    }
}

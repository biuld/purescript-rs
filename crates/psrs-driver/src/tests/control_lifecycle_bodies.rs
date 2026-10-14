//! Semantic helper IDs cannot replace proofs of their actual extent behavior.
use super::control_checkpoints::{SOURCE, binding};
use psrs_backend::cc::{Assignment, AssignmentKind as A, ControlHelperRole as R};
use psrs_backend::mir::{BasicBlock, Instruction as I, Terminator as T};

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
                errors
                    .iter()
                    .any(|e| e.pass == "P8 CC lifecycle body" || e.pass == "P8 CC verification"),
                "{role:?}/{mutation}: {errors:?}"
            );
        }
    }
}

#[test]
fn mir_lifecycle_bodies_reject_identity_wrong_status_and_missing_close() {
    for role in [R::Check, R::Close] {
        for mutation in 0..3 {
            let input = crate::lower_main_to_cc(SOURCE).unwrap();
            let (symbol, _) = binding(&input.cc.representations, role);
            let mut module = psrs_backend::mir::lower_module(input.cc).unwrap().0;
            let function = module
                .functions
                .iter_mut()
                .find(|f| f.symbol == symbol)
                .unwrap();
            match mutation {
                0 => {
                    function.result = function.parameters[0];
                    function.blocks = vec![BasicBlock {
                        id: function.entry,
                        parameters: vec![],
                        instructions: vec![],
                        terminator: Some(T::Return {
                            value: function.result,
                            span: function.span,
                        }),
                    }];
                }
                1 => {
                    let I::Constant { value, .. } = function.blocks.iter_mut().flat_map(|b| &mut b.instructions).find(|i| matches!(i, I::Constant { value, .. } if *value == if role == R::Check {0} else {1})).unwrap() else { unreachable!() };
                    *value = 2;
                }
                _ if role == R::Close => {
                    for block in &mut function.blocks {
                        block
                            .instructions
                            .retain(|i| !matches!(i, I::ArraySet { .. }));
                    }
                }
                _ => {
                    for block in &mut function.blocks {
                        if let Some(T::Jump { arguments, .. }) = &mut block.terminator
                            && arguments.len() == 1
                        {
                            arguments[0] = function.parameters[0];
                        }
                    }
                }
            }
            let errors = psrs_backend::mir::verify_module(&module)
                .expect_err("malformed physical lifecycle body");
            assert!(
                errors.iter().any(|e| e.pass == "P9 MIR lifecycle body"),
                "{role:?}/{mutation}: {errors:?}"
            );
        }
    }
}

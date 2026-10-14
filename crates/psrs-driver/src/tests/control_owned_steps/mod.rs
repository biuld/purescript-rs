//! One-shot transfer must be complete, ordered and free of extra callbacks.
use super::control_checkpoints::{SOURCE, binding};
use psrs_backend::cc::ControlHelperRole as R;

mod cc;
mod mir;

#[derive(Clone, Copy, Debug)]
enum Mutation {
    Guard,
    GuardTag,
    Running,
    RunningValue,
    Clear,
    ClearValue,
    Consumed,
    ConsumedValue,
    StateOrder,
    ReadOrder,
    Callee,
    Answer,
    Result,
    ExtraCall,
    ExtraStore,
}
const MUTATIONS: &[Mutation] = &[
    Mutation::Guard,
    Mutation::GuardTag,
    Mutation::Running,
    Mutation::RunningValue,
    Mutation::Clear,
    Mutation::ClearValue,
    Mutation::Consumed,
    Mutation::ConsumedValue,
    Mutation::StateOrder,
    Mutation::ReadOrder,
    Mutation::Callee,
    Mutation::Answer,
    Mutation::Result,
    Mutation::ExtraCall,
    Mutation::ExtraStore,
];

#[test]
fn cc_owned_execution_requires_the_complete_transfer() {
    for mutation in MUTATIONS {
        cc::check(Some(*mutation), false);
    }
}
#[test]
fn mir_owned_execution_requires_the_complete_transfer() {
    for mutation in MUTATIONS {
        mir::check(Some(*mutation), false);
    }
}
#[test]
fn cc_owned_pending_guard_accepts_integer_equality() {
    cc::check(None, true);
}
#[test]
fn mir_owned_pending_guard_accepts_integer_equality() {
    mir::check(None, true);
}

#[test]
fn cc_private_cells_are_checked_at_publication() {
    use psrs_backend::cc::{Assignment, AssignmentKind as A, ValueDecl, ValueId, ValueShape};
    for role in [R::Start, R::Once] {
        for final_value in [0, 2] {
            let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
            let (symbol, _) = binding(&input.cc.representations, role);
            let function = input
                .cc
                .functions
                .iter_mut()
                .find(|function| function.symbol == symbol)
                .unwrap();
            let (cell, representation) = function
                .assignments
                .iter()
                .find_map(|assignment| match &assignment.kind {
                    A::ArrayNew {
                        representation,
                        elements,
                        ..
                    } if elements.len() == 1
                        && function.values.iter().any(|value| {
                            value.id == elements[0] && value.ty == ValueShape::Integer
                        }) =>
                    {
                        Some((assignment.destination, *representation))
                    }
                    _ => None,
                })
                .unwrap();
            let fresh = ValueId(
                function
                    .values
                    .iter()
                    .map(|value| value.id.0)
                    .max()
                    .unwrap()
                    + 1,
            );
            function.values.extend([
                ValueDecl {
                    id: fresh,
                    ty: ValueShape::Integer,
                },
                ValueDecl {
                    id: ValueId(fresh.0 + 1),
                    ty: ValueShape::Integer,
                },
            ]);
            function.assignments.extend([
                Assignment {
                    destination: fresh,
                    kind: A::Constant(final_value),
                    span: function.span,
                },
                Assignment {
                    destination: ValueId(fresh.0 + 1),
                    kind: A::Constant(0),
                    span: function.span,
                },
                Assignment {
                    destination: cell,
                    kind: A::ArraySet {
                        destination: cell,
                        representation,
                        value: cell,
                        index: ValueId(fresh.0 + 1),
                        new_value: fresh,
                    },
                    span: function.span,
                },
            ]);
            let result = psrs_backend::cc::verify_module(&input.cc);
            if final_value == 0 {
                result.expect("live published cell after an in-place write");
            } else {
                let errors = result.unwrap_err();
                assert!(
                    errors
                        .iter()
                        .any(|error| error.pass == "P8 CC control initialization"),
                    "{role:?}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn mir_private_cells_are_checked_at_publication() {
    use psrs_backend::mir::Instruction as I;
    use psrs_backend::types::{ValueDecl, ValueId, ValueType};
    for role in [R::Start, R::Once] {
        for final_value in [0, 2] {
            let input = crate::lower_main_to_cc(SOURCE).unwrap();
            let (symbol, _) = binding(&input.cc.representations, role);
            // Mutate retained helper bodies before generic inlining/reachability.
            let mut module = psrs_backend::mir::lower_module(input.cc).unwrap().0;
            let function = module
                .functions
                .iter_mut()
                .find(|function| function.symbol == symbol)
                .unwrap();
            let (cell, type_index) = function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .find_map(|instruction| match instruction {
                    I::ArrayNew {
                        destination,
                        type_index,
                        elements,
                        ..
                    } if elements.len() == 1
                        && function.values.iter().any(|value| {
                            value.id == elements[0]
                                && matches!(value.ty, ValueType::I32 | ValueType::Boolean)
                        }) =>
                    {
                        Some((*destination, *type_index))
                    }
                    I::ArrayNewFilled {
                        destination,
                        type_index,
                        value,
                        ..
                    } if function.values.iter().any(|decl| {
                        decl.id == *value && matches!(decl.ty, ValueType::I32 | ValueType::Boolean)
                    }) =>
                    {
                        Some((*destination, *type_index))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{role:?} factory: {function:#?}"));
            let fresh = ValueId(
                function
                    .values
                    .iter()
                    .map(|value| value.id.0)
                    .max()
                    .unwrap()
                    + 1,
            );
            function.values.extend([
                ValueDecl {
                    id: fresh,
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(fresh.0 + 1),
                    ty: ValueType::I32,
                },
            ]);
            let block = function
                .blocks
                .iter_mut()
                .find(|block| {
                    block
                        .instructions
                        .iter()
                        .any(|instruction| instruction.destination() == Some(cell))
                })
                .unwrap();
            block.instructions.extend([
                I::Constant {
                    destination: fresh,
                    value: final_value,
                    span: function.span,
                },
                I::Constant {
                    destination: ValueId(fresh.0 + 1),
                    value: 0,
                    span: function.span,
                },
                I::ArraySet {
                    type_index,
                    value: cell,
                    index: ValueId(fresh.0 + 1),
                    new_value: fresh,
                    span: function.span,
                },
            ]);
            let result = psrs_backend::mir::verify_module(&module);
            if final_value == 0 {
                result.expect("live published cell after an in-place write");
            } else {
                let errors = result.unwrap_err();
                assert!(
                    errors
                        .iter()
                        .any(|error| error.pass == "P9 MIR control initialization"),
                    "{role:?}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn cc_cancellation_requires_pending_cancelled_and_clear() {
    use psrs_backend::cc::AssignmentKind as A;
    for mutation in 0..3 {
        let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
        let (symbol, _) = binding(&input.cc.representations, R::Cancel);
        let function = input
            .cc
            .functions
            .iter_mut()
            .find(|function| function.symbol == symbol)
            .unwrap();
        match mutation {
            0 => {
                function
                    .assignments
                    .retain(|assignment| !matches!(assignment.kind, A::TagSwitch { .. }));
            }
            1 => {
                let assignment = function
                    .assignments
                    .iter_mut()
                    .find(|assignment| matches!(assignment.kind, A::Constant(3)))
                    .unwrap();
                assignment.kind = A::Constant(2);
            }
            _ => {
                let last = function
                    .assignments
                    .iter()
                    .rposition(|assignment| matches!(assignment.kind, A::ArraySet { .. }))
                    .unwrap();
                function.assignments.remove(last);
            }
        }
        let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.pass == "P8 CC control initialization"),
            "{mutation}: {errors:?}"
        );
    }
}

#[test]
fn mir_cancellation_requires_pending_cancelled_and_clear() {
    use psrs_backend::mir::{Instruction as I, Terminator as T};
    for mutation in 0..3 {
        let input = crate::lower_main_to_cc(SOURCE).unwrap();
        let (symbol, _) = binding(&input.cc.representations, R::Cancel);
        // Mutate retained helper bodies before generic inlining/reachability.
        let mut module = psrs_backend::mir::lower_module(input.cc).unwrap().0;
        let function = module
            .functions
            .iter_mut()
            .find(|function| function.symbol == symbol)
            .unwrap();
        match mutation {
            0 => {
                let block = function
                    .blocks
                    .iter_mut()
                    .find(|block| matches!(block.terminator, Some(T::Switch { .. })))
                    .unwrap();
                let T::Switch { cases, span, .. } = block.terminator.take().unwrap() else {
                    unreachable!()
                };
                block.terminator = Some(T::Jump {
                    target: cases[0].1,
                    arguments: vec![],
                    span,
                });
            }
            1 => {
                let I::Constant { value, .. } = function
                    .blocks
                    .iter_mut()
                    .flat_map(|block| &mut block.instructions)
                    .find(|instruction| matches!(instruction, I::Constant { value: 3, .. }))
                    .unwrap()
                else {
                    unreachable!()
                };
                *value = 2;
            }
            _ => {
                let block = function
                    .blocks
                    .iter_mut()
                    .find(|block| {
                        block
                            .instructions
                            .iter()
                            .any(|instruction| matches!(instruction, I::ArraySet { .. }))
                    })
                    .unwrap();
                let last = block
                    .instructions
                    .iter()
                    .rposition(|instruction| matches!(instruction, I::ArraySet { .. }))
                    .unwrap();
                block.instructions.remove(last);
            }
        }
        let errors = psrs_backend::mir::verify_module(&module).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.pass == "P9 MIR control initialization"),
            "{mutation}: {errors:?}"
        );
    }
}

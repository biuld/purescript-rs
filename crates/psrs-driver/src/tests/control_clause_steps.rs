//! Required scoped steps consume their payload and feed the next computation.
use super::control_checkpoints::{SOURCE, binding};
use super::control_dispatch::{carrier_repr, visit};
use psrs_backend::cc::{AssignmentKind as A, ControlHelperRole as R};
use psrs_backend::mir::{Instruction as I, Terminator as T};

#[derive(Clone, Copy)]
enum Mutation {
    Omit,
    Payload,
    Discard,
    Resumption,
    DeadCall,
    State,
    RootHandler,
}

fn cc_step(tag: i32, mutation: Mutation, message: &str) {
    let mut module = crate::lower_main_to_cc(SOURCE).unwrap().cc;
    let context = carrier_repr(&module.representations, R::Executor, 5);
    let (symbol, _) = binding(&module.representations, R::Executor);
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    let parameters = function.parameters.clone();
    let A::TagSwitch { cases, .. } = &mut function.assignments.last_mut().unwrap().kind else {
        panic!("input dispatch")
    };
    let arm = &mut cases
        .iter_mut()
        .find(|case| case.tag == tag)
        .unwrap()
        .assignments;
    // Locate the current context's scoped branch through its tag projection.
    let mut selector = None;
    visit(arm, &mut |a| {
        if let A::VariantTag { representation, .. } = a.kind
            && representation == context
        {
            selector = Some(a.destination);
            return true;
        }
        false
    });
    let selector = selector.unwrap();
    let changed = visit(arm, &mut |a| {
        let A::TagSwitch { value, cases, .. } = &mut a.kind else {
            return false;
        };
        if *value != selector {
            return false;
        }
        let scoped = cases.iter_mut().find(|case| case.tag == 1).unwrap();
        match mutation {
            Mutation::DeadCall => unreachable!("MIR-only mutation"),
            Mutation::State | Mutation::RootHandler => {
                let A::IndirectCall {
                    function: callee,
                    arguments: operands,
                    ..
                } = &scoped
                    .assignments
                    .iter()
                    .find(|a| matches!(a.kind, A::IndirectCall { .. }))
                    .unwrap()
                    .kind
                else {
                    unreachable!()
                };
                let (index, operand) = if matches!(mutation, Mutation::State) {
                    (0, operands[0])
                } else {
                    (1, *callee)
                };
                let A::DirectCall { arguments, .. } = &mut scoped
                    .assignments
                    .iter_mut()
                    .find(|a| a.destination == scoped.value)
                    .unwrap()
                    .kind
                else {
                    panic!("executor transition")
                };
                arguments[index] = operand;
            }
            Mutation::Resumption => {
                let A::IndirectCall { arguments, .. } = &mut scoped
                    .assignments
                    .iter_mut()
                    .rfind(|a| matches!(a.kind, A::IndirectCall { .. }))
                    .unwrap()
                    .kind
                else {
                    unreachable!()
                };
                arguments[0] = parameters[0];
            }
            Mutation::Discard => {
                let A::DirectCall { arguments, .. } = &mut scoped
                    .assignments
                    .iter_mut()
                    .find(|a| a.destination == scoped.value)
                    .unwrap()
                    .kind
                else {
                    panic!("executor transition")
                };
                arguments[2] = parameters[2];
            }
            Mutation::Payload => {
                let A::IndirectCall { arguments, .. } = &mut scoped
                    .assignments
                    .iter_mut()
                    .find(|a| matches!(a.kind, A::IndirectCall { .. }))
                    .unwrap()
                    .kind
                else {
                    unreachable!()
                };
                arguments[0] = parameters[0];
            }
            Mutation::Omit => {
                let call = scoped
                    .assignments
                    .iter_mut()
                    .find(|a| matches!(a.kind, A::IndirectCall { .. }))
                    .unwrap();
                let A::IndirectCall { arguments, .. } = &call.kind else {
                    unreachable!()
                };
                // A well-typed erased-value copy replaces the invocation.
                let reference = match function
                    .values
                    .iter()
                    .find(|v| v.id == call.destination)
                    .unwrap()
                    .ty
                {
                    psrs_backend::cc::ValueShape::Reference(reference) => reference,
                    _ => panic!("erased call result"),
                };
                call.kind = A::RepresentationCast {
                    destination: call.destination,
                    value: arguments[0],
                    reference,
                };
            }
        }
        true
    });
    assert!(changed);
    let errors = psrs_backend::cc::verify_module(&module).expect_err("broken scoped step");
    assert!(
        errors.iter().any(|e| e.message.contains(message)),
        "{errors:?}"
    );
}

fn mir_step(tag: i32, mutation: Mutation, message: &str) {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (symbol, _) = binding(&input.cc.representations, R::Executor);
    let mut module = crate::compile_main_stages(SOURCE).unwrap().mir;
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    let (dispatch, target) = function
        .blocks
        .iter()
        .find_map(|b| match &b.terminator {
            Some(T::Switch { cases, .. })
                if cases.iter().map(|(tag, _)| *tag).collect::<Vec<_>>()
                    == vec![0, 1, 2, 3, 4, 5] =>
            {
                Some((b.id, cases.iter().find(|(t, _)| *t == tag).unwrap().1))
            }
            _ => None,
        })
        .unwrap();
    let mut pending = vec![target];
    let mut seen = std::collections::HashSet::new();
    let mut scoped = None;
    while let Some(id) = pending.pop() {
        if id == dispatch || !seen.insert(id) {
            continue;
        }
        let b = function.blocks.iter().find(|b| b.id == id).unwrap();
        match &b.terminator {
            Some(T::Switch { cases, default, .. }) => {
                if cases.iter().map(|(t, _)| *t).collect::<Vec<_>>() == vec![0, 1] {
                    scoped = cases.iter().find(|(t, _)| *t == 1).map(|(_, id)| *id);
                    break;
                }
                pending.extend(cases.iter().map(|(_, id)| *id));
                pending.push(*default);
            }
            Some(T::Jump { target, .. }) => pending.push(*target),
            _ => {}
        }
    }
    let scoped = scoped.unwrap();
    let block = function.blocks.iter_mut().find(|b| b.id == scoped).unwrap();
    match mutation {
        Mutation::State | Mutation::RootHandler => {
            let I::ClosureCall {
                function: callee,
                arguments: operands,
                ..
            } = block
                .instructions
                .iter()
                .find(|i| matches!(i, I::ClosureCall { .. }))
                .unwrap()
            else {
                unreachable!()
            };
            let (index, operand) = if matches!(mutation, Mutation::State) {
                (0, operands[0])
            } else {
                (1, *callee)
            };
            let T::Jump { arguments, .. } = block.terminator.as_mut().unwrap() else {
                panic!("loop transition")
            };
            arguments[index] = operand;
        }
        Mutation::DeadCall => {
            let index = block
                .instructions
                .iter()
                .position(|i| matches!(i, I::ClosureCall { .. }))
                .unwrap();
            let call = block.instructions.remove(index);
            let id = psrs_backend::mir::BlockId(
                function.blocks.iter().map(|b| b.id.0).max().unwrap() + 1,
            );
            function.blocks.push(psrs_backend::mir::BasicBlock {
                id,
                parameters: vec![],
                instructions: vec![call],
                terminator: Some(T::Trap {
                    span: function.span,
                }),
            });
        }
        Mutation::Resumption => {
            let I::ClosureCall { arguments, .. } = block
                .instructions
                .iter_mut()
                .rfind(|i| matches!(i, I::ClosureCall { .. }))
                .unwrap()
            else {
                unreachable!()
            };
            arguments[0] = function.parameters[0];
        }
        Mutation::Discard => {
            let T::Jump { arguments, .. } = block.terminator.as_mut().unwrap() else {
                panic!("loop transition")
            };
            arguments[2] = function.parameters[2];
        }
        Mutation::Payload => {
            let I::ClosureCall { arguments, .. } = block
                .instructions
                .iter_mut()
                .find(|i| matches!(i, I::ClosureCall { .. }))
                .unwrap()
            else {
                unreachable!()
            };
            arguments[0] = function.parameters[0];
        }
        Mutation::Omit => {
            let call = block
                .instructions
                .iter_mut()
                .find(|i| matches!(i, I::ClosureCall { .. }))
                .unwrap();
            let I::ClosureCall {
                destination,
                arguments,
                span,
                ..
            } = call
            else {
                unreachable!()
            };
            *call = I::Copy {
                destination: *destination,
                value: arguments[0],
                span: *span,
            };
        }
    }
    let errors = psrs_backend::mir::verify_module(&module).expect_err("broken scoped step");
    assert!(
        errors.iter().any(|e| e.message.contains(message)),
        "{errors:?}"
    );
}

#[test]
fn cc_scoped_steps_require_the_clause_application_result() {
    for tag in [0, 1] {
        for mutation in [Mutation::Omit, Mutation::Discard] {
            cc_step(
                tag,
                mutation,
                "source clause result must enter the next executor step",
            );
        }
    }
}
#[test]
fn mir_scoped_steps_require_the_clause_application_result() {
    for tag in [0, 1] {
        for mutation in [Mutation::Omit, Mutation::Discard] {
            mir_step(
                tag,
                mutation,
                "source clause result must enter the next executor step",
            );
        }
    }
}
#[test]
fn cc_scoped_steps_consume_the_current_source_payload() {
    for tag in [0, 1] {
        cc_step(
            tag,
            Mutation::Payload,
            "source clause must consume the current computation payload",
        );
    }
}
#[test]
fn mir_scoped_steps_consume_the_current_source_payload() {
    for tag in [0, 1] {
        mir_step(
            tag,
            Mutation::Payload,
            "source clause must consume the current computation payload",
        );
    }
}

#[test]
fn cc_operation_step_supplies_the_raw_resumption() {
    cc_step(
        1,
        Mutation::Resumption,
        "source operation clause must receive its raw resumption",
    );
}

#[test]
fn mir_operation_step_supplies_the_raw_resumption() {
    mir_step(
        1,
        Mutation::Resumption,
        "source operation clause must receive its raw resumption",
    );
}

#[test]
fn mir_scoped_step_cannot_use_a_clause_call_from_a_dead_block() {
    for tag in [0, 1] {
        mir_step(
            tag,
            Mutation::DeadCall,
            "source clause application must execute in its scoped step",
        );
    }
}

#[test]
fn cc_scoped_steps_preserve_state_and_root_handler() {
    for tag in [0, 1] {
        for mutation in [Mutation::State, Mutation::RootHandler] {
            cc_step(
                tag,
                mutation,
                "source clause transition must preserve its state and root handler",
            );
        }
    }
}

#[test]
fn mir_scoped_steps_preserve_state_and_root_handler() {
    for tag in [0, 1] {
        for mutation in [Mutation::State, Mutation::RootHandler] {
            mir_step(
                tag,
                mutation,
                "source clause transition must preserve its state and root handler",
            );
        }
    }
}

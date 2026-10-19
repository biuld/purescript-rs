//! Dispatch, Bind/Resume, Finish and helper-call obligations at the P8 boundary.
//! Historical owners: control_dispatch, control_clause_steps, control_execution_steps,
//! control_finish_provenance and control_helper_contracts at `01f7e66c`.
use super::{SOURCE, binding};
use psrs_backend::cc::{Assignment, AssignmentKind as A, ControlHelperRole as R, Function};

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
            A::Protect { body, handler, .. } => {
                if visit(&mut body.assignments, mutation)
                    || visit(&mut handler.assignments, mutation)
                {
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

fn expect_boundary(input: &psrs_backend::BackendInput) {
    let errors = psrs_backend::cc::verify_module(&input.cc)
        .expect_err("malformed machine step must fail P8");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("emitted machine operands or ordered operations")
            || error
                .message
                .contains("control helper call is missing its semantic contract")
            || error
                .message
                .contains("disagrees with its predeclared semantic helper")),
        "{errors:?}"
    );
}

fn executor(input: &mut psrs_backend::BackendInput) -> &mut Function {
    let (symbol, _) = binding(&input.cc.representations, R::Executor);
    input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .expect("Executor")
}

fn arm(function: &mut Function, tag: i32) -> &mut Vec<Assignment> {
    let A::TagSwitch { cases, .. } = &mut function.assignments.last_mut().unwrap().kind else {
        panic!("executor dispatches on the computation");
    };
    &mut cases
        .iter_mut()
        .find(|case| case.tag == tag)
        .expect("computation arm")
        .assignments
}

fn first_switch_arm(body: &mut [Assignment], tag: i32) -> &mut Vec<Assignment> {
    for assignment in body {
        if let A::TagSwitch { cases, .. } = &mut assignment.kind {
            return &mut cases
                .iter_mut()
                .find(|case| case.tag == tag)
                .unwrap_or_else(|| panic!("dispatch arm {tag}"))
                .assignments;
        }
    }
    panic!("no nested dispatch")
}

#[test]
fn cc_executor_dispatch_default_must_trap() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let function = executor(&mut input);
    let A::TagSwitch {
        default_assignments,
        default_value,
        ..
    } = &mut function.assignments.last_mut().unwrap().kind
    else {
        panic!("computation dispatch");
    };
    default_assignments.clear();
    *default_value = function.parameters[0];
    expect_boundary(&input);
}

#[test]
fn cc_executor_cannot_return_state_instead_of_the_dispatch_result() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let function = executor(&mut input);
    function.result = function.parameters[0];
    expect_boundary(&input);
}

#[test]
fn cc_bind_cannot_reenter_the_current_computation() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (executor_symbol, _) = binding(&input.cc.representations, R::Executor);
    let function = executor(&mut input);
    let current = function.parameters[2];
    let body = arm(function, 2);
    assert!(visit(body, &mut |kind| {
        if let A::DirectCall {
            function,
            arguments,
            ..
        } = kind
            && *function == executor_symbol
        {
            arguments[2] = current;
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

#[test]
fn cc_bind_cannot_replace_its_callback_with_the_root_handler() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let function = executor(&mut input);
    let handler = function.parameters[1];
    let body = arm(function, 2);
    assert!(visit(body, &mut |kind| {
        if let A::VariantNew {
            case: 1, fields, ..
        } = kind
            && fields.len() == 2
        {
            fields[0] = handler;
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

#[test]
fn cc_resume_cannot_replace_replay_activation_with_the_current_extent() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (executor_symbol, _) = binding(&input.cc.representations, R::Executor);
    let function = executor(&mut input);
    let current = function.parameters[4];
    let body = arm(function, 3);
    assert!(visit(body, &mut |kind| {
        if let A::DirectCall {
            function,
            arguments,
            ..
        } = kind
            && *function == executor_symbol
        {
            arguments[4] = current;
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

#[test]
fn cc_finish_cannot_close_the_parent_instead_of_its_extent() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (close, _) = binding(&input.cc.representations, R::Close);
    let function = executor(&mut input);
    let body = first_switch_arm(arm(function, 0), 3);
    let parent = body
        .iter()
        .find_map(|assignment| match &assignment.kind {
            A::VariantGet { field: 2, .. } => Some(assignment.destination),
            _ => None,
        })
        .expect("Finish parent projection");
    assert!(visit(body, &mut |kind| {
        if let A::DirectCall {
            function,
            arguments,
            ..
        } = kind
            && *function == close
        {
            arguments[0] = parent;
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

#[test]
fn cc_finish_cannot_restore_the_closed_extent_as_its_parent() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (executor_symbol, _) = binding(&input.cc.representations, R::Executor);
    let (close, _) = binding(&input.cc.representations, R::Close);
    let function = executor(&mut input);
    let body = first_switch_arm(arm(function, 0), 3);
    let closed = body
        .iter()
        .find_map(|assignment| match &assignment.kind {
            A::DirectCall {
                function,
                arguments,
                ..
            } if *function == close => Some(arguments[0]),
            _ => None,
        })
        .expect("Finish close");
    assert!(visit(body, &mut |kind| {
        if let A::DirectCall {
            function,
            arguments,
            ..
        } = kind
            && *function == executor_symbol
        {
            arguments[4] = closed;
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

#[test]
fn cc_scoped_return_cannot_discard_the_clause_result() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (executor_symbol, _) = binding(&input.cc.representations, R::Executor);
    let function = executor(&mut input);
    let current = function.parameters[2];
    let empty = first_switch_arm(arm(function, 0), 0);
    let body = first_switch_arm(empty, 1);
    assert!(visit(body, &mut |kind| {
        if let A::DirectCall {
            function,
            arguments,
            ..
        } = kind
            && *function == executor_symbol
        {
            arguments[2] = current;
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

#[test]
fn cc_helper_call_cannot_drop_its_semantic_contract() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (check, _) = binding(&input.cc.representations, R::Check);
    let function = executor(&mut input);
    assert!(visit(&mut function.assignments, &mut |kind| {
        if let A::DirectCall {
            function, contract, ..
        } = kind
            && *function == check
        {
            *contract = None;
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

#[test]
fn cc_control_evidence_rejects_a_dangling_contract() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let mut found = false;
    for function in &mut input.cc.functions {
        if visit(&mut function.assignments, &mut |kind| {
            if let A::ControlEvidence { contract, .. } = kind {
                *contract = psrs_backend::cc::ControlContractId(u32::MAX);
                return true;
            }
            false
        }) {
            found = true;
            break;
        }
    }
    assert!(found);
    let errors = psrs_backend::cc::verify_module(&input.cc)
        .expect_err("a dangling source contract must fail CC verification");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("no checked source contract")),
        "{errors:?}"
    );
}

#[test]
fn cc_finish_cannot_exchange_close_for_check() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (close, _) = binding(&input.cc.representations, R::Close);
    let (check, check_id) = binding(&input.cc.representations, R::Check);
    let function = executor(&mut input);
    let body = first_switch_arm(arm(function, 0), 3);
    assert!(visit(body, &mut |kind| {
        if let A::DirectCall {
            function, contract, ..
        } = kind
            && *function == close
        {
            *function = check;
            *contract = Some(check_id);
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

//! Checkpoint, raw-resume and factory-status obligations at the P8 boundary.
//! Historical owners: `control_checkpoints`, `control_checkpoint_ordering`,
//! `control_inlined_publication` at `01f7e66c`.
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
        .expect_err("malformed checkpoint construction must fail P8");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("emitted machine operands or ordered operations")),
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

fn resume_arm(function: &mut Function) -> &mut Vec<Assignment> {
    let A::TagSwitch { cases, .. } = &mut function.assignments.last_mut().unwrap().kind else {
        panic!("executor dispatches on the computation");
    };
    &mut cases
        .iter_mut()
        .find(|case| case.tag == 3)
        .expect("Resume arm")
        .assignments
}

#[test]
fn cc_perform_cannot_capture_the_execution_extent_as_its_checkpoint() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let function = executor(&mut input);
    let execution = function.parameters[4];
    assert!(visit(&mut function.assignments, &mut |kind| {
        if let A::FunctionRef { captures, .. } = kind {
            assert_ne!(captures[1], execution);
            captures[1] = execution;
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

#[test]
fn cc_raw_resume_must_check_its_captured_checkpoint_before_the_frame() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let raw = input
        .cc
        .representations
        .control_resumption()
        .unwrap()
        .function();
    let (check, _) = binding(&input.cc.representations, R::Check);
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == raw)
        .expect("raw resume");
    let before = function.assignments.len();
    function.assignments.retain(|assignment| {
        !matches!(assignment.kind, A::DirectCall { function, .. } if function == check)
    });
    assert_eq!(function.assignments.len(), before - 1);
    expect_boundary(&input);
}

#[test]
fn cc_resume_cannot_check_the_current_execution_extent() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (check, _) = binding(&input.cc.representations, R::Check);
    let function = executor(&mut input);
    let execution = function.parameters[4];
    let body = resume_arm(function);
    assert!(visit(body, &mut |kind| {
        if let A::DirectCall {
            function,
            arguments,
            ..
        } = kind
            && *function == check
        {
            arguments[0] = execution;
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

#[test]
fn cc_resume_cannot_start_replay_before_the_checkpoint_check() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (check, _) = binding(&input.cc.representations, R::Check);
    let (clone, _) = binding(&input.cc.representations, R::CloneSequence);
    let function = executor(&mut input);
    let body = resume_arm(function);
    let check_at = body
        .iter()
        .position(|assignment| {
            matches!(assignment.kind, A::DirectCall { function, .. } if function == check)
        })
        .expect("Resume checks its checkpoint");
    let check_assignment = body.remove(check_at);
    let clone_at = body
        .iter()
        .position(|assignment| {
            matches!(assignment.kind, A::DirectCall { function, .. } if function == clone)
        })
        .expect("Resume clones the saved queue");
    body.insert(clone_at + 1, check_assignment);
    expect_boundary(&input);
}

#[test]
fn cc_start_and_once_status_cells_must_be_initialized_live() {
    for role in [R::Start, R::Once] {
        let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
        let (symbol, _) = binding(&input.cc.representations, role);
        let function = input
            .cc
            .functions
            .iter_mut()
            .find(|function| function.symbol == symbol)
            .unwrap();
        assert!(visit(&mut function.assignments, &mut |kind| {
            if let A::Constant(value) = kind
                && *value == 0
            {
                *value = 2;
                return true;
            }
            false
        }));
        expect_boundary(&input);
    }
}

#[test]
fn cc_handle_cannot_install_its_fresh_extent_as_the_caller_parent() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let function = executor(&mut input);
    assert!(visit(&mut function.assignments, &mut |kind| {
        if let A::VariantNew {
            case: 1, fields, ..
        } = kind
            && fields.len() == 6
            && fields[3] != fields[5]
        {
            fields.swap(3, 5);
            return true;
        }
        false
    }));
    expect_boundary(&input);
}

#[test]
fn cc_raw_resume_accepts_a_same_shape_alias_of_its_checkpoint() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let raw = input
        .cc
        .representations
        .control_resumption()
        .unwrap()
        .function();
    let (check, _) = binding(&input.cc.representations, R::Check);
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == raw)
        .unwrap();
    let check_at = function
        .assignments
        .iter()
        .position(|assignment| {
            matches!(assignment.kind, A::DirectCall { function, .. } if function == check)
        })
        .unwrap();
    let checkpoint = match &function.assignments[check_at].kind {
        A::DirectCall { arguments, .. } => arguments[0],
        _ => unreachable!(),
    };
    let mut declaration = function
        .values
        .iter()
        .find(|value| value.id == checkpoint)
        .unwrap()
        .clone();
    declaration.id = psrs_backend::cc::ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    let psrs_backend::cc::ValueShape::Reference(reference) = declaration.ty else {
        unreachable!()
    };
    function.assignments.insert(
        check_at,
        Assignment {
            destination: declaration.id,
            kind: A::RepresentationCast {
                destination: declaration.id,
                value: checkpoint,
                reference,
            },
            span: function.span,
        },
    );
    match &mut function.assignments[check_at + 1].kind {
        A::DirectCall { arguments, .. } => arguments[0] = declaration.id,
        _ => unreachable!(),
    }
    function.values.push(declaration);
    psrs_backend::cc::verify_module(&input.cc).unwrap();
}

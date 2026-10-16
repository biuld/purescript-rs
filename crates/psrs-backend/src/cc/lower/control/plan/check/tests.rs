use super::super::ir::{Op, State};
use super::*;
use crate::control::schema::{ComputationCase as C, ContextCase as X, SequenceCase as Q};

fn build(program: Program) -> Exec {
    match program {
        Program::Executor => super::super::executor(),
        Program::Run => super::super::run::program(),
        Program::CloneSequence => super::super::clone_sequence(),
        Program::RebuildSequence => super::super::rebuild_sequence(),
        Program::Check => super::super::boundary_check(),
        Program::Close => super::super::boundary_close(),
        Program::Start => super::super::start_boundary(),
        Program::Once => super::super::once(),
        Program::Cancel => super::super::cancel(),
        Program::RawResume => super::super::raw_resume(),
    }
}

fn edit(plan: &mut Exec, change: &mut impl FnMut(&mut Op) -> bool) -> bool {
    match plan {
        Exec::Steps { ops, next } => ops.iter_mut().any(&mut *change) || edit(next, change),
        Exec::Switch { arms, .. } => arms.iter_mut().any(|(_, arm)| edit(arm, change)),
        _ => false,
    }
}

fn reject_op(program: Program, message: &str, change: impl FnMut(&mut Op) -> bool) {
    let mut plan = build(program);
    assert!(edit(&mut plan, &mut { change }));
    let error = check_for(program, &plan).unwrap_err();
    assert!(error.contains(message), "{error}");
}

fn drop_path(exec: &mut Exec, path: &[Selection]) {
    match exec {
        Exec::Steps { next, .. } => drop_path(next, path),
        Exec::Switch { case, arms, .. } => {
            let index = arms
                .iter()
                .position(|(tag, _)| selection(*case, *tag).unwrap() == path[0])
                .unwrap();
            if path.len() == 1 {
                arms.remove(index);
            } else {
                drop_path(&mut arms[index].1, &path[1..]);
            }
        }
        _ => {
            assert!(path.is_empty());
            *exec = Exec::Tail {
                role: crate::cc::ControlHelperRole::Close,
                args: Vec::new(),
            };
        }
    }
}

#[test]
fn every_required_action_is_checked_including_helpers_and_rejection_arms() {
    let mut unique = HashSet::new();
    for action in ActionId::ALL {
        let (program, path) = action.selection();
        assert!(
            unique.insert((program, path)),
            "duplicate registry path for {action:?}"
        );
        let mut plan = build(program);
        check_for(program, &plan).unwrap();
        drop_path(&mut plan, path);
        assert!(
            check_for(program, &plan).is_err(),
            "missing {action:?} was accepted"
        );
    }
    for (program, expected) in [
        (Program::Executor, 20),
        (Program::Run, 1),
        (Program::CloneSequence, 7),
        (Program::RebuildSequence, 4),
        (Program::Check, 2),
        (Program::Close, 2),
        (Program::Start, 1),
        (Program::Once, 1),
        (Program::Cancel, 1),
        (Program::RawResume, 1),
    ] {
        assert_eq!(
            ActionId::ALL
                .into_iter()
                .filter(|a| a.selection().0 == program)
                .count(),
            expected
        );
    }
}

#[test]
fn scoped_actions_cannot_disappear_behind_complete_root_actions() {
    let mut plan = build(Program::Executor);
    drop_path(
        &mut plan,
        &[
            Selection::Computation(C::Perform),
            Selection::Context(X::Scoped),
        ],
    );
    assert!(check(&plan).is_err());
}

#[test]
fn projections_require_the_selected_case_of_their_own_source() {
    reject_op(Program::Executor, "case refinement", |op| {
        if let Op::Project {
            source: Operand::State(State::Sequence),
            case,
            ..
        } = op
            && *case == Case::Sequence(Q::Cons)
        {
            *case = Case::Sequence(Q::Append);
            return true;
        }
        false
    });
}

#[test]
fn forward_and_out_of_range_operands_are_rejected_before_expansion() {
    for source in [Operand::Temp(0), Operand::Temp(999), Operand::Param(6)] {
        reject_op(Program::Executor, "lexical scope", |op| {
            if let Op::Project { source: actual, .. } = op {
                *actual = source.clone();
                return true;
            }
            false
        });
    }
}

#[test]
fn branch_local_temporaries_cannot_escape_to_siblings() {
    let mut plan = build(Program::CloneSequence);
    let Exec::Switch { arms, .. } = &mut plan else {
        unreachable!()
    };
    let Exec::Switch { arms, .. } = &mut arms[0].1 else {
        unreachable!()
    };
    let Exec::Tail { args, .. } = &mut arms[0].1 else {
        unreachable!()
    };
    args[0] = Operand::Temp(0);
    assert_eq!(
        check_for(Program::CloneSequence, &plan),
        Err("machine plan operand is outside its lexical scope")
    );
}

#[test]
fn allocation_and_helper_parameters_keep_their_logical_roles() {
    reject_op(Program::Executor, "logical storage role", |op| {
        if let Op::Node {
            case: Case::Sequence(Q::Cons),
            fields,
        } = op
        {
            fields[1] = Operand::State(State::Extent);
            return true;
        }
        false
    });
    reject_op(Program::Executor, "wrong arity", |op| {
        if let Op::Helper { args, .. } = op {
            args.pop();
            return true;
        }
        false
    });
}

#[test]
fn helper_cannot_inherit_executor_state() {
    reject_op(Program::Once, "executor state", |op| {
        if let Op::NewCallbackCell { value } = op {
            *value = Operand::State(State::Handler);
            return true;
        }
        false
    });
}

#[test]
fn capture_slots_and_product_indices_have_checked_domains() {
    reject_op(Program::RawResume, "capture index", |op| {
        if let Op::Capture { index, .. } = op {
            *index = 2;
            return true;
        }
        false
    });
    reject_op(Program::Executor, "pair field", |op| {
        if let Op::PairField { field, .. } = op {
            *field = 2;
            return true;
        }
        false
    });
    reject_op(Program::Executor, "replay result field", |op| {
        if let Op::ReplayField { field, .. } = op {
            *field = 2;
            return true;
        }
        false
    });
}

#[test]
fn lifecycle_status_values_have_a_checked_domain() {
    reject_op(Program::Cancel, "lifecycle domain", |op| {
        if let Op::SetStatus { value, .. } = op {
            *value = 4;
            return true;
        }
        false
    });
}

#[test]
fn equal_signature_tail_calls_keep_their_semantic_role() {
    let mut plan = build(Program::Check);
    let Exec::Switch { arms, .. } = &mut plan else {
        unreachable!()
    };
    let Exec::Steps { next, .. } = &mut arms[1].1 else {
        unreachable!()
    };
    let Exec::Tail { role, .. } = next.as_mut() else {
        unreachable!()
    };
    *role = crate::cc::ControlHelperRole::Close;
    assert_eq!(
        check_for(Program::Check, &plan),
        Err("machine plan action has the wrong terminal outcome")
    );
}

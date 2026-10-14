//! Independent structural validation of the driver plan.

use super::ir::{Case, Exec, Op};
use crate::control::execution::ActionId;
use crate::control::schema::{ComputationCase, ContextCase, SequenceCase};
use std::collections::HashSet;

///
/// It verifies that every dispatch covers exactly its schema's cases, that
/// every allocation has the declared field arity, and that every required
/// flat-driver action is present. It does not inspect operation order; the
/// story's own CC verifiers check the emitted body.
pub(crate) fn check(exec: &Exec) -> Result<(), &'static str> {
    verify(exec)?;
    let mut actions = HashSet::new();
    record_executor(exec, &mut actions)?;
    if ActionId::ALL.iter().all(|action| actions.contains(action)) {
        Ok(())
    } else {
        Err("checked driver plan is missing a required action")
    }
}

pub(crate) fn verify(exec: &Exec) -> Result<(), &'static str> {
    match exec {
        Exec::Steps { ops, next } => {
            for op in ops {
                if let Op::Node { case, fields } = op
                    && fields.len() != case.field_count()
                {
                    return Err("driver plan allocates a node with the wrong field arity");
                }
            }
            verify(next)
        }
        Exec::Switch { case, arms, .. } => {
            let mut tags = arms.iter().map(|(tag, _)| *tag).collect::<Vec<_>>();
            let unique = tags.len();
            tags.sort_unstable();
            tags.dedup();
            if tags.len() != unique {
                return Err("driver plan dispatch repeats a case");
            }
            let allowed = case.tags();
            if tags.iter().any(|tag| !allowed.contains(tag)) {
                return Err("driver plan dispatch invents a schema case");
            }
            for (_, arm) in arms {
                verify(arm)?;
            }
            Ok(())
        }
        Exec::Advance { .. }
        | Exec::Publish { .. }
        | Exec::Return { .. }
        | Exec::Tail { .. }
        | Exec::Reject { .. } => Ok(()),
    }
}

/// Navigate the recognized driver shape and record each reached action.
fn record_executor(exec: &Exec, actions: &mut HashSet<ActionId>) -> Result<(), &'static str> {
    let arms = switch_arms(exec, &|case| matches!(case, Case::Computation(_)))
        .ok_or("driver plan must refine the computation carrier first")?;
    for (tag, arm) in arms {
        if *tag == ComputationCase::Return.tag() {
            record_return(arm, actions)?;
        } else if *tag == ComputationCase::Perform.tag()
            && switch_arms(arm, &|case| matches!(case, Case::Context(_)))
                .is_some_and(|arms| arms.iter().any(|(tag, _)| *tag == ContextCase::Root.tag()))
        {
            actions.insert(ActionId::PerformRoot);
        }
    }
    Ok(())
}

fn record_return(exec: &Exec, actions: &mut HashSet<ActionId>) -> Result<(), &'static str> {
    let queue = switch_arms(exec, &|case| matches!(case, Case::Sequence(_)))
        .ok_or("return must refine the queue")?;
    for (tag, arm) in queue {
        if *tag == SequenceCase::Empty.tag()
            && switch_arms(arm, &|case| matches!(case, Case::Context(_)))
                .is_some_and(|arms| arms.iter().any(|(t, _)| *t == ContextCase::Root.tag()))
        {
            actions.insert(ActionId::ReturnEmptyRoot);
        } else if *tag == SequenceCase::Cons.tag() {
            actions.insert(ActionId::ReturnCons);
        } else if *tag == SequenceCase::Finish.tag() {
            actions.insert(ActionId::ReturnFinish);
        } else if *tag == SequenceCase::Append.tag()
            && let Some(left) = switch_arms(arm, &|case| matches!(case, Case::Sequence(_)))
        {
            for (left_tag, _) in left {
                let action = match *left_tag {
                    tag if tag == SequenceCase::Empty.tag() => Some(ActionId::AppendEmpty),
                    tag if tag == SequenceCase::Cons.tag() => Some(ActionId::AppendCons),
                    tag if tag == SequenceCase::Append.tag() => Some(ActionId::AppendAppend),
                    tag if tag == SequenceCase::Finish.tag() => Some(ActionId::AppendFinish),
                    _ => None,
                };
                if let Some(action) = action {
                    actions.insert(action);
                }
            }
        }
    }
    Ok(())
}

/// The first switch in this subtree whose carrier satisfies `wanted`.
fn switch_arms<'a>(exec: &'a Exec, wanted: &dyn Fn(Case) -> bool) -> Option<&'a [(u32, Exec)]> {
    match exec {
        Exec::Switch { case, arms, .. } if wanted(*case) => Some(arms),
        Exec::Steps { ops: _, next } => switch_arms(next, wanted),
        Exec::Switch { arms, .. } => {
            for (_, arm) in arms {
                if let Some(found) = switch_arms(arm, wanted) {
                    return Some(found);
                }
            }
            None
        }
        Exec::Advance { .. }
        | Exec::Publish { .. }
        | Exec::Return { .. }
        | Exec::Tail { .. }
        | Exec::Reject { .. } => None,
    }
}

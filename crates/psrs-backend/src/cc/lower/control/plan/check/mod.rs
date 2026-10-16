//! Check all named actions and typed, lexically scoped construction operands.
mod operations;
mod scope;
#[cfg(test)]
mod tests;

use super::ir::{Case, Exec, Operand, state_index};
use crate::control::execution::{ActionId, Outcome, Program, Selection};
use crate::control::schema::{FieldKind, OwnerCase};
use scope::{Info, Scope};
use std::collections::HashSet;

type Checked<T> = Result<T, &'static str>;

#[cfg(test)]
pub(crate) fn check(exec: &Exec) -> Checked<()> {
    check_for(Program::Executor, exec)
}

pub(crate) fn check_for(program: Program, exec: &Exec) -> Checked<()> {
    let mut reached = HashSet::new();
    walk(exec, &mut Vec::new(), program, &mut reached)?;
    for action in ActionId::ALL {
        if action.selection().0 == program && !reached.contains(&action) {
            return Err("checked machine plan is missing a required action");
        }
    }
    let scope = Scope::new(program);
    scope.exec(exec)
}

fn selection(case: Case, tag: u32) -> Checked<Selection> {
    use crate::control::schema::{
        ComputationCase as C, ContextCase as X, ExtentCase as E, SequenceCase as Q,
    };
    match case {
        Case::Computation(_) => C::ALL
            .into_iter()
            .find(|case| case.tag() == tag)
            .map(Selection::Computation),
        Case::Sequence(_) => Q::ALL
            .into_iter()
            .find(|case| case.tag() == tag)
            .map(Selection::Sequence),
        Case::Context(_) => X::ALL
            .into_iter()
            .find(|case| case.tag() == tag)
            .map(Selection::Context),
        Case::Extent(_) => E::ALL
            .into_iter()
            .find(|case| case.tag() == tag)
            .map(Selection::Extent),
        Case::Owner(_) => None,
    }
    .ok_or("machine plan dispatch invents an action selection")
}

fn walk(
    exec: &Exec,
    path: &mut Vec<Selection>,
    program: Program,
    reached: &mut HashSet<ActionId>,
) -> Checked<()> {
    match exec {
        Exec::Steps { next, .. } => return walk(next, path, program, reached),
        Exec::Switch { case, arms, .. } => {
            let mut tags = arms.iter().map(|(tag, _)| *tag).collect::<Vec<_>>();
            tags.sort_unstable();
            if tags != case.tags() {
                return Err("machine plan dispatch must cover every schema case exactly once");
            }
            for (tag, arm) in arms {
                path.push(selection(*case, *tag)?);
                walk(arm, path, program, reached)?;
                path.pop();
            }
            return Ok(());
        }
        _ => {}
    }
    let action = ActionId::ALL
        .into_iter()
        .find(|action| action.selection() == (program, path.as_slice()))
        .ok_or("machine plan has an undeclared action path")?;
    let outcome = match exec {
        Exec::Advance { .. } => Outcome::Advance,
        Exec::Publish { .. } => Outcome::Publish,
        Exec::Return { .. } => Outcome::Return,
        Exec::Tail { role, .. } => Outcome::Tail(*role),
        Exec::Reject { .. } => Outcome::Reject,
        _ => unreachable!(),
    };
    if action.outcome() != outcome {
        return Err("machine plan action has the wrong terminal outcome");
    }
    if !reached.insert(action) {
        return Err("machine plan repeats an action");
    }
    Ok(())
}

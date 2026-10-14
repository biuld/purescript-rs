//! An independent bounded model of the flat driver's transition traces.
//!
//! `explore` recovers the terminal outcomes the plan reaches, keyed by the path
//! of case refinements. `expected` and `helper_expected` state the same
//! transitions directly from the operational semantics table, not from the plan
//! builders. The tests compare the two so a dropped, duplicated or mis-routed
//! transition is a mismatch rather than a silently passing recognizer.

use super::ir::{Case, Exec};
use super::shape::Terminal;
use crate::control::schema::{ComputationCase, ContextCase, ExtentCase, SequenceCase};

/// One trace: the refinement path and its terminal outcome.
type Trace = (Vec<(Case, u32)>, Terminal);
/// A named plan and the traces the semantics require of it.
type Planned = (&'static str, Exec, Vec<Trace>);

/// The terminal outcome reached by the plan along exactly `path`.
pub(super) fn terminal_of(exec: &Exec, path: &[(Case, u32)]) -> Option<Terminal> {
    match path.split_first() {
        None => leaf(exec),
        Some(((case, tag), rest)) => match exec {
            Exec::Steps { next, .. } => terminal_of(next, path),
            Exec::Switch {
                case: actual, arms, ..
            } if same_family(*actual, *case) => {
                let arm = arms.iter().find(|(candidate, _)| candidate == tag)?;
                terminal_of(&arm.1, rest)
            }
            _ => None,
        },
    }
}

fn leaf(exec: &Exec) -> Option<Terminal> {
    match exec {
        Exec::Steps { next, .. } => leaf(next),
        Exec::Advance { .. } => Some(Terminal::Advance),
        Exec::Publish { .. } | Exec::Return { .. } => Some(Terminal::Value),
        Exec::Tail { .. } => Some(Terminal::Tail),
        Exec::Reject { .. } => Some(Terminal::Reject),
        Exec::Switch { .. } => None,
    }
}

/// A switch declares one case as a family marker; the refined tag is the path.
fn same_family(left: Case, right: Case) -> bool {
    std::mem::discriminant(&left) == std::mem::discriminant(&right)
}

/// The number of terminal traces the plan has.
pub(super) fn trace_count(exec: &Exec) -> usize {
    match exec {
        Exec::Steps { next, .. } => trace_count(next),
        Exec::Switch { arms, .. } => arms.iter().map(|(_, arm)| trace_count(arm)).sum(),
        _ => 1,
    }
}

fn c(case: ComputationCase) -> Case {
    Case::Computation(case)
}
fn s(case: SequenceCase) -> Case {
    Case::Sequence(case)
}
fn x(case: ContextCase) -> Case {
    Case::Context(case)
}
fn e(case: ExtentCase) -> Case {
    Case::Extent(case)
}

/// The transition traces required by the semantics, independent of the builder.
pub(super) fn expected() -> Vec<Trace> {
    use ComputationCase::*;
    use ContextCase::*;
    use SequenceCase::*;
    use Terminal::*;
    vec![
        // Return: publish at the root, or apply the return clause and advance.
        (vec![(c(Return), 0), (s(Empty), 0), (x(Root), 0)], Value),
        (vec![(c(Return), 0), (s(Empty), 0), (x(Scoped), 1)], Advance),
        (vec![(c(Return), 0), (s(Cons), 1)], Advance),
        (vec![(c(Return), 0), (s(Append), 2), (s(Empty), 0)], Advance),
        (vec![(c(Return), 0), (s(Append), 2), (s(Cons), 1)], Advance),
        (
            vec![(c(Return), 0), (s(Append), 2), (s(Append), 2)],
            Advance,
        ),
        (
            vec![(c(Return), 0), (s(Append), 2), (s(Finish), 3)],
            Advance,
        ),
        (vec![(c(Return), 0), (s(Finish), 3)], Advance),
        // Perform: root applies the handler, scoped applies the operation clause.
        (vec![(c(Perform), 1), (x(Root), 0)], Advance),
        (vec![(c(Perform), 1), (x(Scoped), 1)], Advance),
        // Inert frames advance directly.
        (vec![(c(Bind), 2)], Advance),
        (vec![(c(Resume), 3)], Advance),
        // Handle: normalize Append, otherwise enter under a fresh or shared extent.
        (vec![(c(Handle), 4), (s(Empty), 0)], Advance),
        (vec![(c(Handle), 4), (s(Cons), 1)], Advance),
        (vec![(c(Handle), 4), (s(Append), 2), (s(Empty), 0)], Advance),
        (vec![(c(Handle), 4), (s(Append), 2), (s(Cons), 1)], Advance),
        (
            vec![(c(Handle), 4), (s(Append), 2), (s(Append), 2)],
            Advance,
        ),
        (
            vec![(c(Handle), 4), (s(Append), 2), (s(Finish), 3)],
            Advance,
        ),
        (vec![(c(Handle), 4), (s(Finish), 3)], Advance),
        (vec![(c(OwnedResume), 5)], Advance),
    ]
}

/// The helper bodies' required traces, independent of their builders.
pub(super) fn helper_expected() -> Vec<Planned> {
    use ExtentCase::*;
    use SequenceCase::*;
    use Terminal::*;
    vec![
        (
            "clone_sequence",
            super::clone_sequence(),
            vec![
                (vec![(s(Empty), 0), (s(Empty), 0)], Tail),
                (vec![(s(Empty), 0), (s(Append), 2)], Tail),
                (vec![(s(Empty), 0), (s(Cons), 1)], Reject),
                (vec![(s(Empty), 0), (s(Finish), 3)], Reject),
                (vec![(s(Cons), 1)], Tail),
                (vec![(s(Append), 2)], Tail),
                (vec![(s(Finish), 3)], Tail),
            ],
        ),
        (
            "rebuild_sequence",
            super::rebuild_sequence(),
            vec![
                (vec![(s(Empty), 0)], Value),
                (vec![(s(Cons), 1)], Tail),
                (vec![(s(Finish), 3)], Tail),
            ],
        ),
        (
            "boundary_check",
            super::boundary_check(),
            vec![(vec![(e(Root), 0)], Value), (vec![(e(Scoped), 1)], Tail)],
        ),
        (
            "boundary_close",
            super::boundary_close(),
            vec![(vec![(e(Root), 0)], Value), (vec![(e(Scoped), 1)], Value)],
        ),
        (
            "start_boundary",
            super::start_boundary(),
            vec![(vec![], Value)],
        ),
        ("once", super::once(), vec![(vec![], Value)]),
        ("cancel", super::cancel(), vec![(vec![], Value)]),
        ("raw_resume", super::raw_resume(), vec![(vec![], Value)]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_matches_the_independent_model() {
        let plan = super::super::executor();
        let expected = expected();
        assert_eq!(
            trace_count(&plan),
            expected.len(),
            "plan trace count disagrees with the model"
        );
        for (path, terminal) in &expected {
            assert_eq!(
                terminal_of(&plan, path).as_ref(),
                Some(terminal),
                "plan has no matching terminal for {path:?}"
            );
        }
    }

    #[test]
    fn helper_plans_match_the_model() {
        for (name, plan, expected) in helper_expected() {
            assert_eq!(
                trace_count(&plan),
                expected.len(),
                "{name} trace count disagrees with the model"
            );
            for (path, terminal) in &expected {
                assert_eq!(
                    terminal_of(&plan, path).as_ref(),
                    Some(terminal),
                    "{name} has no matching terminal for {path:?}"
                );
            }
        }
    }
}

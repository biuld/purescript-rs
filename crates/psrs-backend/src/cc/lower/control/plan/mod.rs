//! A temporary checked construction plan for the flat driver.
//!
//! The plan names each transition's selected cases, its ordered projections,
//! allocations, applications, helper calls and extent/status operations, and
//! exactly one terminal outcome. It is effect-agnostic and independent of any
//! concrete operation or handler.
//!
//! The executor is *built* from this plan: [`Machine::expand`] emits ordinary CC
//! from the same structure that [`check`] independently validates. An emitter
//! defect therefore fails the structural check before the generic CC verifiers
//! see a body at all.
//!
//! The module is split by responsibility: [`ir`] names the plan structure and
//! its construction helpers, [`returns`]/[`driver`]/[`handle`] build the
//! transitions, [`check`] validates the plan and [`expand`] emits ordinary CC.

mod boundary;
mod check;
mod driver;
mod expand;
mod handle;
mod helpers;
mod ir;
#[cfg(test)]
mod model;
mod returns;
mod shape;

pub(crate) use boundary::check_boundary;
pub(super) use check::check;
pub(super) use check::verify as verify_plan;
pub(super) use helpers::{
    boundary_check, boundary_close, cancel, clone_sequence, once, raw_resume, rebuild_sequence,
    start_boundary,
};
pub(crate) use ir::{Exec as PlanExec, Family};
pub(crate) use shape::{Shape, shape as shape_of};

use crate::control::schema::ComputationCase;
use ir::{Case, Cursor, Exec, State, state, switch};

/// Build the complete flat-driver program.
pub(super) fn executor() -> Exec {
    let cursor = Cursor::new();
    let arms = ComputationCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = cursor.clone();
            let arm = match case {
                ComputationCase::Return => returns::returned(&mut cursor),
                ComputationCase::Perform => driver::perform(&mut cursor),
                ComputationCase::Bind => driver::bind(&mut cursor),
                ComputationCase::Resume => driver::resume(&mut cursor),
                ComputationCase::Handle => handle::handle(&mut cursor),
                ComputationCase::OwnedResume => handle::owned_resume(&mut cursor),
            };
            (case.tag(), arm)
        })
        .collect();
    switch(
        state(State::Computation),
        Case::Computation(ComputationCase::Return),
        arms,
    )
}

#[cfg(test)]
mod tests {
    use super::ir::{Case, Exec, Op, Operand, Shape, State};
    use super::*;
    use crate::control::schema::{ComputationCase, SequenceCase};

    #[test]
    fn executor_plan_is_complete() {
        check(&executor()).expect("the driver plan covers every required action");
    }

    #[test]
    fn helper_plans_are_well_formed() {
        for plan in [
            clone_sequence(),
            rebuild_sequence(),
            boundary_check(),
            boundary_close(),
            start_boundary(),
            once(),
            cancel(),
            raw_resume(),
        ] {
            verify_plan(&plan).expect("helper plan must be well-formed");
        }
    }

    #[test]
    fn dispatch_rejects_an_invented_case() {
        let plan = Exec::Switch {
            source: Operand::State(State::Computation),
            case: Case::Computation(ComputationCase::Return),
            arms: vec![(
                99,
                Exec::Publish {
                    result: Operand::State(State::Value),
                },
            )],
        };
        assert!(check(&plan).is_err());
    }

    #[test]
    fn dispatch_rejects_a_duplicate_case() {
        let plan = Exec::Switch {
            source: Operand::State(State::Computation),
            case: Case::Computation(ComputationCase::Return),
            arms: vec![
                (
                    0,
                    Exec::Reject {
                        shape: Shape::ReplayResult,
                    },
                ),
                (
                    0,
                    Exec::Reject {
                        shape: Shape::ReplayResult,
                    },
                ),
            ],
        };
        assert!(verify_plan(&plan).is_err());
    }

    #[test]
    fn node_field_arity_is_checked() {
        let plan = Exec::Steps {
            ops: vec![Op::Node {
                case: Case::Sequence(SequenceCase::Cons),
                fields: vec![Operand::Param(0)],
            }],
            next: Box::new(Exec::Return {
                value: Operand::Param(0),
            }),
        };
        assert!(verify_plan(&plan).is_err());
    }

    #[test]
    fn dropping_an_action_fails_the_check() {
        let mut plan = executor();
        if let Exec::Switch { arms, .. } = &mut plan {
            arms.retain(|(tag, _)| *tag != ComputationCase::Perform.tag());
        }
        assert!(check(&plan).is_err());
    }
}

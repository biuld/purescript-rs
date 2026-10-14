//! Executor transitions are terminal: their results must reach the actual exit.
use crate::BackendError;
use crate::cc::value_identity::{definitions, reference_origin, transparent};
use crate::cc::{
    Assignment, AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable, ValueId,
};
use std::collections::{HashMap, HashSet};

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    if !table
        .control_helpers
        .iter()
        .any(|helper| helper.function() == function.symbol && helper.role() == R::Executor)
    {
        return Ok(());
    }
    let definitions = definitions(&function.assignments);
    body(
        function,
        &function.assignments,
        function.result,
        &definitions,
    )
    .map(|_| ())
    .map_err(|message| {
        vec![
            BackendError::invalid_ir("P8 CC executor exit", function.span, message)
                .with_module(function.symbol.module),
        ]
    })
}

fn body(
    function: &Function,
    assignments: &[Assignment],
    result: ValueId,
    definitions: &HashMap<ValueId, &A>,
) -> Result<bool, &'static str> {
    let mut transitions = false;
    for assignment in assignments {
        let transition = match &assignment.kind {
            A::DirectCall {
                function: target, ..
            } => *target == function.symbol,
            A::If {
                then_assignments,
                then_value,
                else_assignments,
                else_value,
                ..
            } => {
                let left = body(function, then_assignments, *then_value, definitions)?;
                let right = body(function, else_assignments, *else_value, definitions)?;
                left || right
            }
            A::TagSwitch {
                cases,
                default_assignments,
                default_value,
                ..
            } => {
                let mut found = body(function, default_assignments, *default_value, definitions)?;
                for case in cases {
                    found |= body(function, &case.assignments, case.value, definitions)?;
                }
                found
            }
            _ => false,
        };
        if transition
            && !forwards(
                result,
                assignment.destination,
                definitions,
                &mut HashSet::new(),
            )
        {
            return Err("executor transition result must reach every normal exit");
        }
        transitions |= transition;
    }
    Ok(transitions)
}

/// Check the returned value graph, never call arguments or unrelated effects.
/// The requirement applies to every normal alternative, including joins whose
/// inputs differ; a trap has no returned value.
fn forwards(
    value: ValueId,
    required: ValueId,
    definitions: &HashMap<ValueId, &A>,
    active: &mut HashSet<ValueId>,
) -> bool {
    if value == required || reference_origin(value, |v| definitions.get(&v).copied()) == required {
        return true;
    }
    if !active.insert(value) {
        return false;
    }
    let result = match definitions.get(&value).copied() {
        Some(A::Unreachable) => true,
        Some(A::RepresentationCast { value, .. }) => {
            forwards(*value, required, definitions, active)
        }
        Some(A::AggregateConvert {
            value, conversion, ..
        }) if transparent(&conversion.plan) => forwards(*value, required, definitions, active),
        Some(A::If {
            then_value,
            else_value,
            ..
        }) => {
            forwards(*then_value, required, definitions, active)
                && forwards(*else_value, required, definitions, active)
        }
        Some(A::TagSwitch {
            cases,
            default_value,
            ..
        }) => {
            cases
                .iter()
                .all(|case| forwards(case.value, required, definitions, active))
                && forwards(*default_value, required, definitions, active)
        }
        _ => false,
    };
    active.remove(&value);
    result
}

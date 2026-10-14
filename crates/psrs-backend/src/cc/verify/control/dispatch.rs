//! Machine dispatch coverage and projections are checked on actual CC paths.
use crate::BackendError;
use crate::cc::branch_facts::{Facts, selected, tag_source};
use crate::cc::{AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable};
use crate::control::dispatch::Carrier;

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    let Some(role) = table
        .control_helpers
        .iter()
        .find(|h| h.function() == function.symbol)
        .map(|h| h.role())
    else {
        return Ok(());
    };
    if !matches!(role, R::Executor | R::CloneSequence | R::RebuildSequence) {
        return Ok(());
    }
    let schema = table
        .control_checkpoint_schema()
        .ok_or_else(|| error(function, "control dispatch has no checked schema"))?;
    let queue = table
        .control_sequence()
        .ok_or_else(|| error(function, "control dispatch has no checked queue schema"))?;
    let classify = |repr| {
        if repr == schema.computation {
            Some(Carrier::Computation)
        } else if repr == schema.context {
            Some(Carrier::Context)
        } else if repr == queue {
            Some(if role == R::RebuildSequence {
                Carrier::Reverse
            } else {
                Carrier::Queue
            })
        } else {
            None
        }
    };
    let definitions = crate::cc::value_identity::definitions(&function.assignments);
    let origin =
        |v| crate::cc::value_identity::reference_origin(v, |v| definitions.get(&v).copied());
    let index = if role == R::Executor { 2 } else { 0 };
    let input = function.parameters.get(index).copied().ok_or_else(|| {
        error(
            function,
            "control dispatch has an invalid parameter contract",
        )
    })?;
    let entry_repr = if role == R::Executor {
        schema.computation
    } else {
        queue
    };
    let Some(A::TagSwitch { value, .. }) = definitions.get(&origin(function.result)).copied()
    else {
        return Err(error(
            function,
            "control helper result must be its current input dispatch",
        ));
    };
    if tag_source(*value, &definitions) != Some((entry_repr, origin(input))) {
        return Err(error(
            function,
            "control helper result must be its current input dispatch",
        ));
    }
    let mut pending = vec![(function.assignments.as_slice(), Facts::new())];
    while let Some((body, facts)) = pending.pop() {
        for assignment in body {
            match &assignment.kind {
                A::VariantGet {
                    representation,
                    case,
                    value,
                    ..
                } if classify(*representation).is_some() => {
                    // Only a proven selection can contradict the projected case.
                    // An unprovable source is left to the relation-specific
                    // verifiers rather than rejected here.
                    if let Some(known) = selected(*representation, *value, &facts, &definitions)
                        && i32::try_from(*case).ok() != Some(known)
                    {
                        return Err(error(
                            function,
                            "control projection must match the selected case of its actual input",
                        ));
                    }
                }
                A::TagSwitch {
                    value,
                    cases,
                    default_assignments,
                    default_value,
                    ..
                } => {
                    let selector = tag_source(*value, &definitions);
                    if let Some((repr, owner)) = selector
                        && let Some(carrier) = classify(repr)
                    {
                        carrier
                            .verify(cases.iter().map(|case| case.tag))
                            .map_err(|m| error(function, m))?;
                        if !matches!(
                            definitions.get(&origin(*default_value)).copied(),
                            Some(A::Unreachable)
                        ) {
                            return Err(error(
                                function,
                                "control dispatch must trap on unsupported alternatives",
                            ));
                        }
                        if role == R::Executor
                            && repr == schema.computation
                            && owner != origin(input)
                        {
                            return Err(error(
                                function,
                                "computation dispatch must select the current executor input",
                            ));
                        }
                    }
                    for case in cases {
                        let mut branch = facts.clone();
                        if let Some(key) = selector {
                            branch.insert(key, case.tag);
                        }
                        pending.push((case.assignments.as_slice(), branch));
                    }
                    // The default proves none of the explicitly selected tags.
                    let mut fallback = facts.clone();
                    if let Some(key) = selector {
                        fallback.remove(&key);
                    }
                    pending.push((default_assignments.as_slice(), fallback));
                }
                A::If {
                    then_assignments,
                    else_assignments,
                    ..
                } => pending.extend([
                    (then_assignments.as_slice(), facts.clone()),
                    (else_assignments.as_slice(), facts.clone()),
                ]),
                _ => {}
            }
        }
    }
    Ok(())
}

fn error(function: &Function, message: &'static str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P8 CC control dispatch", function.span, message)
            .with_module(function.symbol.module),
    ]
}

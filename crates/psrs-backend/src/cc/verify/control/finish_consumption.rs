//! Read actual CC projections and tail transitions, independently of the builder.
use crate::BackendError;
use crate::cc::{
    AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable, ValueId,
};
use crate::control::provenance::{FinishProjection, finish_relation};
use std::collections::HashMap;

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    if !table
        .control_helpers
        .iter()
        .any(|helper| helper.role() == R::Executor && helper.function() == function.symbol)
    {
        return Ok(());
    }
    let sequence = table.control_sequence().ok_or_else(|| {
        error(
            function,
            function.span,
            "control executor has no checked sequence schema",
        )
    })?;
    let definitions = crate::cc::value_identity::definitions(&function.assignments);
    let mut pending = vec![function.assignments.as_slice()];
    while let Some(body) = pending.pop() {
        for (index, assignment) in body.iter().enumerate() {
            match &assignment.kind {
                A::TagSwitch {
                    cases,
                    default_assignments,
                    ..
                } => {
                    pending.extend(cases.iter().map(|case| case.assignments.as_slice()));
                    pending.push(default_assignments);
                }
                A::DirectCall {
                    contract: Some(contract),
                    arguments,
                    ..
                } if table
                    .control_helpers
                    .get(contract.0 as usize)
                    .is_some_and(|helper| helper.role() == R::Close) =>
                {
                    let next = body[index + 1..]
                        .iter()
                        .find_map(|assignment| match &assignment.kind {
                            A::DirectCall {
                                contract: Some(contract),
                                arguments,
                                ..
                            } if table
                                .control_helpers
                                .get(contract.0 as usize)
                                .is_some_and(|helper| helper.role() == R::Executor) =>
                            {
                                Some(arguments)
                            }
                            _ => None,
                        })
                        .ok_or_else(|| {
                            error(
                                function,
                                assignment.span,
                                "control finish has no executor transition",
                            )
                        })?;
                    if arguments.len() != 1 || next.len() != 6 {
                        return Err(error(
                            function,
                            assignment.span,
                            "control finish transition has an invalid arity",
                        ));
                    }
                    finish_relation(
                        projection(arguments[0], &definitions, sequence),
                        projection(next[3], &definitions, sequence),
                        projection(next[4], &definitions, sequence),
                    )
                    .map_err(|message| error(function, assignment.span, message))?;
                }
                A::DirectCall {
                    contract: Some(contract),
                    arguments,
                    ..
                } if table
                    .control_helpers
                    .get(contract.0 as usize)
                    .is_some_and(|helper| helper.role() == R::Executor)
                    && arguments.len() == 6
                    && projection(arguments[3], &definitions, sequence)
                        .is_some_and(|projection| projection.field == 1) =>
                {
                    let close = body[..index]
                        .iter()
                        .rev()
                        .find_map(|assignment| match &assignment.kind {
                            A::DirectCall {
                                contract: Some(contract),
                                arguments,
                                ..
                            } if table
                                .control_helpers
                                .get(contract.0 as usize)
                                .is_some_and(|helper| helper.role() == R::Close) =>
                            {
                                arguments.first().copied()
                            }
                            _ => None,
                        })
                        .ok_or_else(|| {
                            error(
                                function,
                                assignment.span,
                                "control finish transition does not close its frame",
                            )
                        })?;
                    finish_relation(
                        projection(close, &definitions, sequence),
                        projection(arguments[3], &definitions, sequence),
                        projection(arguments[4], &definitions, sequence),
                    )
                    .map_err(|message| error(function, assignment.span, message))?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn projection(
    value: ValueId,
    definitions: &HashMap<ValueId, &A>,
    sequence: crate::cc::ReprId,
) -> Option<FinishProjection> {
    let definition = |value| definitions.get(&value).copied();
    let value = crate::cc::value_identity::reference_origin(value, definition);
    match definition(value)? {
        A::VariantGet {
            representation,
            case: 3,
            field,
            value: owner,
            ..
        } if *representation == sequence => Some(FinishProjection {
            owner: crate::cc::value_identity::reference_origin(*owner, definition),
            field: *field,
        }),
        _ => None,
    }
}

fn error(
    function: &Function,
    span: psrs_span::TextRange,
    message: &'static str,
) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P8 CC control provenance", span, message)
            .with_module(function.symbol.module),
    ]
}

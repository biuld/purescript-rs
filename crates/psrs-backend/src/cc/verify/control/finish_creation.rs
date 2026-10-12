//! Check target completion records and immutable finish reassociation.
use crate::BackendError;
use crate::cc::{AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable};
use crate::control_provenance::{FinishProjection, context_finish_relation};

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
    if function.parameters.len() != 6 {
        return Err(error(
            function,
            "finish producer has an invalid executor contract",
        ));
    }
    let context = table
        .control_checkpoint_schema()
        .ok_or_else(|| error(function, "finish producer has no checked context schema"))?
        .context;
    let sequence = table
        .control_sequence()
        .ok_or_else(|| error(function, "finish producer has no checked queue schema"))?;
    let definitions = crate::cc::value_identity::definitions(&function.assignments);
    let origin = |value| {
        crate::cc::value_identity::reference_origin(value, |value| definitions.get(&value).copied())
    };
    let projection = |value, representation, case| match definitions.get(&origin(value)).copied() {
        Some(A::VariantGet {
            representation: actual,
            case: tag,
            field,
            value,
            ..
        }) if *actual == representation && *tag == case => Some(FinishProjection {
            owner: origin(*value),
            field: *field,
        }),
        _ => None,
    };
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
                A::If {
                    then_assignments,
                    else_assignments,
                    ..
                } => pending.extend([then_assignments.as_slice(), else_assignments.as_slice()]),
                A::VariantNew {
                    representation,
                    case: 3,
                    fields,
                    ..
                } if *representation == sequence => {
                    if fields.len() != 3 {
                        return Err(error(
                            function,
                            "finish producer has an invalid frame schema",
                        ));
                    }
                    if projection(fields[0], context, 1).is_some() {
                        context_finish_relation(
                            function.parameters[5],
                            projection(fields[0], context, 1),
                            projection(fields[1], context, 1),
                            projection(fields[2], context, 1),
                        )
                        .map_err(|message| error(function, message))?;
                        let next = body[index + 1..]
                            .iter()
                            .find_map(|assignment| match &assignment.kind {
                                A::DirectCall {
                                    function: target,
                                    arguments,
                                    ..
                                } if *target == function.symbol => Some(arguments),
                                _ => None,
                            })
                            .ok_or_else(|| {
                                error(function, "target finish has no executor transition")
                            })?;
                        if next.len() != 6
                            || origin(next[3]) != assignment.destination
                            || origin(next[4]) != origin(fields[0])
                            || projection(next[5], context, 1)
                                != Some(FinishProjection {
                                    owner: function.parameters[5],
                                    field: 4,
                                })
                        {
                            return Err(error(
                                function,
                                "target finish must enter its caller context with the saved extent",
                            ));
                        }
                    } else {
                        let extent = projection(fields[0], sequence, 3)
                            .filter(|projection| projection.field == 0)
                            .ok_or_else(|| {
                                error(function, "finish copy must preserve its original extent")
                            })?;
                        if projection(fields[2], sequence, 3)
                            != Some(FinishProjection {
                                owner: extent.owner,
                                field: 2,
                            })
                        {
                            return Err(error(
                                function,
                                "finish copy must preserve its original parent",
                            ));
                        }
                        let Some(A::VariantNew {
                            representation,
                            case: 2,
                            fields: appended,
                            ..
                        }) = definitions.get(&origin(fields[1])).copied()
                        else {
                            return Err(error(
                                function,
                                "finish copy must reassociate the original queue",
                            ));
                        };
                        if *representation != sequence
                            || appended.len() != 2
                            || projection(appended[0], sequence, 3)
                                != Some(FinishProjection {
                                    owner: extent.owner,
                                    field: 1,
                                })
                        {
                            return Err(error(
                                function,
                                "finish copy must reassociate the original queue",
                            ));
                        }
                        let left = projection(extent.owner, sequence, 2)
                            .filter(|projection| projection.field == 0)
                            .ok_or_else(|| {
                                error(function, "finish copy has no original append input")
                            })?;
                        if left.owner != function.parameters[3]
                            || projection(appended[1], sequence, 2)
                                != Some(FinishProjection {
                                    owner: left.owner,
                                    field: 1,
                                })
                        {
                            return Err(error(
                                function,
                                "finish copy must retain the original append tail",
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn error(function: &Function, message: &'static str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P8 CC finish creation", function.span, message)
            .with_module(function.symbol.module),
    ]
}

//! Source Handle entry retains its clauses, caller work and lifetime boundary.
use crate::BackendError;
use crate::cc::{AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable};
use crate::control::provenance::{
    FinishProjection, context_clause_relation, context_tail_relation,
};

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
    let parameters = &function.parameters;
    if parameters.len() != 6 {
        return Err(error(
            function,
            "source context has an invalid executor contract",
        ));
    }
    let checkpoint = table
        .control_checkpoint_schema()
        .ok_or_else(|| error(function, "source context has no checked schema"))?;
    let schema = table
        .control_replay_schema()
        .ok_or_else(|| error(function, "source context has no checked lifecycle schema"))?;
    let definitions = crate::cc::value_identity::definitions(&function.assignments);
    let origin = |value| {
        crate::cc::value_identity::reference_origin(value, |value| definitions.get(&value).copied())
    };
    let constant = |value, expected| {
        matches!(definitions.get(&origin(value)).copied(),
        Some(A::Constant(value)) if *value == expected)
    };
    let projection = |value, expected, tag| match definitions.get(&origin(value)).copied() {
        Some(A::VariantGet {
            representation,
            case,
            field,
            value,
            ..
        }) if *representation == expected && *case == tag => Some(FinishProjection {
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
                    case: 1,
                    fields,
                    ..
                } if *representation == checkpoint.context => {
                    if fields.len() != 6 {
                        return Err(error(
                            function,
                            "source context has an invalid frame schema",
                        ));
                    }
                    context_clause_relation(
                        parameters[2],
                        projection(fields[0], checkpoint.computation, 4),
                        projection(fields[1], checkpoint.computation, 4),
                    )
                    .map_err(|message| error(function, message))?;
                    if origin(fields[4]) != parameters[5] {
                        return Err(error(
                            function,
                            "source context must retain its current caller context",
                        ));
                    }
                    if projection(fields[5], schema.sequence, 3).is_some() {
                        context_tail_relation(
                            parameters[3],
                            projection(fields[2], schema.sequence, 3),
                            projection(fields[3], schema.sequence, 3),
                            projection(fields[5], schema.sequence, 3),
                        )
                        .map_err(|message| error(function, message))?;
                    } else {
                        if origin(fields[2]) != parameters[3] || origin(fields[3]) != parameters[4]
                        {
                            return Err(error(
                                function,
                                "source context must save its current queue and execution parent",
                            ));
                        }
                        let Some(A::VariantNew {
                            representation,
                            case: 1,
                            fields: activation,
                            ..
                        }) = definitions.get(&origin(fields[5])).copied()
                        else {
                            return Err(error(
                                function,
                                "source context must allocate a fresh child extent",
                            ));
                        };
                        if *representation != schema.boundary
                            || activation.len() != 2
                            || origin(activation[1]) != parameters[4]
                        {
                            return Err(error(
                                function,
                                "source context extent must retain its current execution parent",
                            ));
                        }
                        let initialized = match definitions.get(&origin(activation[0])).copied() {
                            Some(A::ArrayNew {
                                representation,
                                elements,
                                ..
                            }) => {
                                *representation == schema.status
                                    && elements.len() == 1
                                    && constant(elements[0], 0)
                            }
                            Some(A::ArrayFill {
                                representation,
                                length,
                                value,
                                ..
                            }) => {
                                *representation == schema.status
                                    && constant(*length, 1)
                                    && constant(*value, 0)
                            }
                            _ => false,
                        };
                        if !initialized {
                            return Err(error(
                                function,
                                "source context extent needs a fresh live status cell in its iteration",
                            ));
                        }
                    }
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
                            error(function, "source context has no executor transition")
                        })?;
                    let empty = matches!(next.get(3).and_then(|value| definitions.get(&origin(*value))).copied(),
                        Some(A::VariantNew { representation, case: 0, fields, .. })
                            if *representation == schema.sequence && fields.is_empty());
                    if next.len() != 6
                        || origin(next[0]) != parameters[0]
                        || origin(next[1]) != parameters[1]
                        || projection(next[2], checkpoint.computation, 4)
                            != Some(FinishProjection {
                                owner: parameters[2],
                                field: 2,
                            })
                        || !empty
                        || origin(next[4]) != origin(fields[5])
                        || origin(next[5]) != assignment.destination
                    {
                        return Err(error(
                            function,
                            "source context transition must enter its Handle body with an empty queue and its own extent",
                        ));
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
        BackendError::invalid_ir("P8 CC context entry", function.span, message)
            .with_module(function.symbol.module),
    ]
}

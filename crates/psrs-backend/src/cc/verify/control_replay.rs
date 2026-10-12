//! Saved finish validation and fresh replay activation ownership.
use crate::BackendError;
use crate::cc::{AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable};
use crate::control_provenance::{
    FinishProjection, replay_activation_relation, saved_finish_relation,
};

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
    functions: Option<&std::collections::HashMap<psrs_hir::SymbolId, &Function>>,
) -> Result<(), Vec<BackendError>> {
    let role = table
        .control_helpers
        .iter()
        .find(|helper| helper.function() == function.symbol)
        .map(|helper| helper.role());
    if !matches!(role, Some(R::CloneSequence | R::RebuildSequence)) {
        return Ok(());
    }
    let read_only = functions
        .map(super::read_only::functions)
        .unwrap_or_default();
    let schema = table
        .control_replay_schema()
        .ok_or_else(|| error(function, "replay has no checked lifecycle schema"))?;
    let expected = if role == Some(R::CloneSequence) { 5 } else { 4 };
    if function.parameters.len() != expected {
        return Err(error(function, "replay has an invalid parameter contract"));
    }
    let parameters = &function.parameters;
    let definitions = crate::cc::value_identity::definitions(&function.assignments);
    let origin = |value| {
        crate::cc::value_identity::reference_origin(value, |value| definitions.get(&value).copied())
    };
    let constant = |value, expected| matches!(definitions.get(&origin(value)).copied(), Some(A::Constant(value)) if *value == expected);
    let projection = |value, expected_case| match definitions.get(&origin(value)).copied() {
        Some(A::VariantGet {
            representation,
            case,
            field,
            value,
            ..
        }) if *representation == schema.sequence && *case == expected_case => {
            Some(FinishProjection {
                owner: origin(*value),
                field: *field,
            })
        }
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
                A::DirectCall {
                    function: target,
                    contract,
                    ..
                } if contract
                    .and_then(|id| table.control_helpers.get(id.0 as usize))
                    .is_none_or(|helper| {
                        !matches!(
                            helper.role(),
                            R::Check | R::CloneSequence | R::RebuildSequence
                        )
                    })
                    && !read_only.contains(target) =>
                {
                    return Err(error(
                        function,
                        "replay traversal cannot invoke callbacks or mutate lifecycle cells",
                    ));
                }
                A::IndirectCall { .. } | A::RuntimeCall { .. } | A::FunctionRef { .. } => {
                    return Err(error(
                        function,
                        "replay traversal cannot invoke callbacks or mutate lifecycle cells",
                    ));
                }
                A::VariantNew {
                    representation,
                    case: 1,
                    fields,
                    ..
                } if *representation == schema.sequence => {
                    let accumulator =
                        parameters[if role == Some(R::CloneSequence) { 2 } else { 1 }];
                    if fields.len() != 2
                        || projection(fields[0], 1)
                            != Some(FinishProjection {
                                owner: parameters[0],
                                field: 0,
                            })
                        || origin(fields[1]) != accumulator
                    {
                        return Err(error(
                            function,
                            "replay must share the original callback and retain its accumulator",
                        ));
                    }
                }
                A::VariantNew {
                    representation,
                    case: 3,
                    fields,
                    ..
                } if *representation == schema.sequence => {
                    if fields.len() != 3 {
                        return Err(error(function, "replay finish has an invalid schema"));
                    }
                    if role == Some(R::CloneSequence) {
                        saved_finish_relation(parameters[0], projection(fields[0], 3))
                            .map_err(|message| error(function, message))?;
                        if origin(fields[1]) != parameters[2] || origin(fields[2]) != parameters[4]
                        {
                            return Err(error(
                                function,
                                "replay traversal must retain its reverse accumulator and parent",
                            ));
                        }
                        if !body[..index].iter().any(|assignment| matches!(&assignment.kind,
                            A::DirectCall { contract: Some(id), arguments, .. }
                                if table.control_helpers.get(id.0 as usize).is_some_and(|helper| helper.role() == R::Check)
                                    && arguments.len() == 1 && origin(arguments[0]) == origin(fields[0]))) {
                            return Err(error(function, "replay must check the saved finish extent before copying it"));
                        }
                    } else {
                        replay_activation_relation(
                            parameters[1],
                            parameters[2],
                            origin(fields[1]),
                            origin(fields[2]),
                        )
                        .map_err(|message| error(function, message))?;
                        let extent = origin(fields[0]);
                        let Some(A::VariantNew {
                            representation,
                            case: 1,
                            fields: activation,
                            ..
                        }) = definitions.get(&extent).copied()
                        else {
                            return Err(error(
                                function,
                                "replay must create a fresh activation extent",
                            ));
                        };
                        if *representation != schema.boundary
                            || activation.len() != 2
                            || origin(activation[1]) != parameters[3]
                        {
                            return Err(error(
                                function,
                                "replay activation must use its current parent",
                            ));
                        }
                        let status = origin(activation[0]);
                        let initialized = match definitions.get(&status).copied() {
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
                        // Common CC scope verification proves these local allocations
                        // dominate the construction; every recursive call is a fresh
                        // invocation, including allocations before the tag switch.
                        if !initialized {
                            return Err(error(
                                function,
                                "replay activation needs a fresh live status cell in its iteration",
                            ));
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
                                error(function, "replay activation has no rebuild transition")
                            })?;
                        if next.len() != 4
                            || origin(next[1]) != assignment.destination
                            || origin(next[2]) != extent
                            || origin(next[3]) != extent
                        {
                            return Err(error(
                                function,
                                "replay must advance its queue, active extent and parent together",
                            ));
                        }
                    }
                }
                A::ProductNew {
                    representation,
                    arguments,
                    ..
                } if role == Some(R::RebuildSequence) && *representation == schema.result => {
                    if arguments.len() != 2
                        || origin(arguments[0]) != parameters[1]
                        || origin(arguments[1]) != parameters[2]
                    {
                        return Err(error(
                            function,
                            "replay result must return its accumulated queue and active extent",
                        ));
                    }
                }
                A::ArraySet { .. } => {
                    return Err(error(
                        function,
                        "replay must not mutate saved or fresh lifecycle cells",
                    ));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn error(function: &Function, message: &'static str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P8 CC control replay", function.span, message)
            .with_module(function.symbol.module),
    ]
}

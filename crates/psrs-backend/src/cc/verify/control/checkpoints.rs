//! Validate actual checkpoint capture, raw-frame construction and replay checks.
use crate::BackendError;
use crate::cc::{
    Assignment, AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable, ValueId,
};
use crate::control::provenance::{
    CaptureProjection, checkpoint_capture_relation, dispatch_checkpoint_relation,
};
use std::collections::HashMap;

type Definitions<'a> = HashMap<ValueId, &'a A>;

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    let Some(raw) = &table.control_resumption else {
        return Ok(());
    };
    let schema = table
        .control_checkpoint_schema()
        .ok_or_else(|| error(function, "raw resumption has no checkpoint schema"))?;
    let definitions = crate::cc::value_identity::definitions(&function.assignments);
    let origin = |value| origin(value, &definitions);
    let executor = table
        .control_helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
        .unwrap()
        .function();
    if function.symbol == executor && function.parameters.len() != 6 {
        return Err(error(
            function,
            "control executor has an invalid parameter contract",
        ));
    }
    if function.symbol == raw.function() {
        if function.parameters.len() != 2 {
            return Err(error(
                function,
                "raw resumption has an invalid parameter contract",
            ));
        }
        let result = origin(function.result);
        let Some(A::VariantNew {
            representation,
            case: 3,
            fields,
            ..
        }) = definitions.get(&result).copied()
        else {
            return Err(error(
                function,
                "raw resumption must return its saved Resume frame",
            ));
        };
        if *representation != schema.computation
            || fields.len() != 3
            || origin(fields[0]) != function.parameters[1]
        {
            return Err(error(
                function,
                "raw resumption frame has an invalid answer or schema",
            ));
        }
        checkpoint_capture_relation(
            function.parameters[0],
            capture(fields[1], &definitions),
            capture(fields[2], &definitions),
        )
        .map_err(|message| error(function, message))?;
        let position = function
            .assignments
            .iter()
            .position(|assignment| assignment.destination == result)
            .ok_or_else(|| {
                error(
                    function,
                    "raw resumption frame has no checked construction sequence",
                )
            })?;
        require_check(
            function,
            table,
            &function.assignments[..position],
            origin(fields[2]),
            &definitions,
        )?;
    }
    let mut pending = vec![function.assignments.as_slice()];
    while let Some(body) = pending.pop() {
        for (index, assignment) in body.iter().enumerate() {
            match &assignment.kind {
                A::If {
                    then_assignments,
                    else_assignments,
                    ..
                } => pending.extend([then_assignments.as_slice(), else_assignments.as_slice()]),
                A::TagSwitch {
                    cases,
                    default_assignments,
                    ..
                } => {
                    pending.extend(cases.iter().map(|case| case.assignments.as_slice()));
                    pending.push(default_assignments);
                }
                A::FunctionRef {
                    function: target,
                    signature,
                    captures,
                } if *target == raw.function() => {
                    if function.symbol != executor
                        || function.parameters.len() != 6
                        || captures.len() != 2
                        || *signature != raw.signature()
                        || origin(captures[0]) != function.parameters[3]
                    {
                        return Err(error(
                            function,
                            "raw resumption factory must capture the executor's current queue",
                        ));
                    }
                    let projection = match definitions.get(&origin(captures[1])).copied() {
                        Some(A::VariantGet {
                            representation,
                            case,
                            field,
                            value,
                            ..
                        }) if *representation == schema.context => {
                            Some((origin(*value), *case, *field))
                        }
                        _ => None,
                    };
                    dispatch_checkpoint_relation(function.parameters[5], projection)
                        .map_err(|message| error(function, message))?;
                }
                A::DirectCall {
                    contract: Some(contract),
                    arguments,
                    ..
                } if function.symbol == executor
                    && table
                        .control_helpers
                        .get(contract.0 as usize)
                        .is_some_and(|helper| helper.role() == R::CloneSequence) =>
                {
                    if arguments.len() != 5 || origin(arguments[3]) != function.parameters[4] {
                        return Err(error(
                            function,
                            "replay must start with its current execution extent",
                        ));
                    }
                    for empty in &arguments[1..3] {
                        if !matches!(definitions.get(&origin(*empty)).copied(), Some(A::VariantNew { representation, case: 0, fields, .. })
                            if Some(*representation) == table.control_sequence() && fields.is_empty())
                        {
                            return Err(error(
                                function,
                                "replay must start with empty traversal accumulators",
                            ));
                        }
                    }
                    let Some(A::TagSwitch {
                        value: tag,
                        cases,
                        default_value,
                        ..
                    }) = definitions.get(&origin(arguments[4])).copied()
                    else {
                        return Err(error(
                            function,
                            "replay parent must select its current dispatch context",
                        ));
                    };
                    if !matches!(definitions.get(&origin(*tag)).copied(), Some(A::VariantTag { representation, value, .. })
                        if *representation == schema.context && origin(*value) == function.parameters[5])
                        || cases.len() != 2
                        || !matches!(
                            definitions.get(&origin(*default_value)).copied(),
                            Some(A::Unreachable)
                        )
                    {
                        return Err(error(
                            function,
                            "replay parent must select its current dispatch context",
                        ));
                    }
                    let mut seen = std::collections::HashSet::new();
                    for case in cases {
                        let projection = match definitions.get(&origin(case.value)).copied() {
                            Some(A::VariantGet {
                                representation,
                                case: selected,
                                field,
                                value,
                                ..
                            }) if *representation == schema.context
                                && *selected as i32 == case.tag =>
                            {
                                Some((origin(*value), *selected, *field))
                            }
                            _ => None,
                        };
                        dispatch_checkpoint_relation(function.parameters[5], projection)
                            .map_err(|message| error(function, message))?;
                        seen.insert(case.tag);
                    }
                    if seen != std::collections::HashSet::from([0, 1]) {
                        return Err(error(
                            function,
                            "replay parent must cover root and scoped dispatch contexts",
                        ));
                    }
                    let Some(saved) = arguments.first() else {
                        return Err(error(function, "replay has no saved queue"));
                    };
                    let Some(A::VariantGet {
                        representation,
                        case: 3,
                        field: 1,
                        value,
                        ..
                    }) = definitions.get(&origin(*saved)).copied()
                    else {
                        return Err(error(
                            function,
                            "replay must use the Resume frame's saved queue",
                        ));
                    };
                    if *representation != schema.computation
                        || origin(*value) != function.parameters[2]
                    {
                        return Err(error(function, "replay must use the current Resume frame"));
                    }
                    let checkpoint = body[..index]
                        .iter()
                        .find_map(|assignment| match &assignment.kind {
                            A::VariantGet {
                                representation,
                                case: 3,
                                field: 2,
                                value: owner,
                                ..
                            } if *representation == schema.computation
                                && origin(*owner) == origin(*value) =>
                            {
                                Some(origin(assignment.destination))
                            }
                            _ => None,
                        })
                        .ok_or_else(|| {
                            error(function, "replay has no current Resume checkpoint")
                        })?;
                    require_check(function, table, &body[..index], checkpoint, &definitions)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn origin(value: ValueId, definitions: &Definitions<'_>) -> ValueId {
    crate::cc::value_identity::reference_origin(value, |value| definitions.get(&value).copied())
}

fn capture(value: ValueId, definitions: &Definitions<'_>) -> Option<CaptureProjection> {
    match definitions.get(&origin(value, definitions)).copied()? {
        A::ClosureGetCapture { closure, index } => Some(CaptureProjection {
            receiver: origin(*closure, definitions),
            slot: *index,
        }),
        _ => None,
    }
}

fn require_check(
    function: &Function,
    table: &RepresentationTable,
    prefix: &[Assignment],
    checkpoint: ValueId,
    definitions: &Definitions<'_>,
) -> Result<(), Vec<BackendError>> {
    if prefix.iter().any(|assignment| matches!(&assignment.kind,
        A::DirectCall { contract: Some(contract), arguments, .. }
            if table.control_helpers.get(contract.0 as usize).is_some_and(|helper| helper.role() == R::Check)
                && arguments.len() == 1 && origin(arguments[0], definitions) == checkpoint)) {
        Ok(())
    } else { Err(error(function, "resumption must check its saved checkpoint before construction or replay")) }
}

fn error(function: &Function, message: &'static str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P8 CC control checkpoint", function.span, message)
            .with_module(function.symbol.module),
    ]
}

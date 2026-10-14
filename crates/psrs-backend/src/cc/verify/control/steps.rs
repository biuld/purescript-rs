//! Required execution transitions consume the current frame's fields.
use crate::BackendError;
use crate::cc::{AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable};
use crate::control::provenance::FinishProjection;
use crate::control::steps::{bind_relation, resume_answer_relation};

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    if !table
        .control_helpers
        .iter()
        .any(|h| h.role() == R::Executor && h.function() == function.symbol)
    {
        return Ok(());
    }
    let result = (|| {
        let schema = table
            .control_checkpoint_schema()
            .ok_or("execution step has no checked schema")?;
        let queue = table
            .control_sequence()
            .ok_or("execution step has no checked queue")?;
        let definitions = crate::cc::value_identity::definitions(&function.assignments);
        let origin =
            |v| crate::cc::value_identity::reference_origin(v, |v| definitions.get(&v).copied());
        let Some(A::TagSwitch { cases, .. }) = definitions.get(&origin(function.result)).copied()
        else {
            return Err("execution step has no input dispatch");
        };
        let parameters = &function.parameters;
        if parameters.len() != 6 {
            return Err("execution step has an invalid executor contract");
        }
        let projection = |value, tag| match definitions.get(&origin(value)).copied() {
            Some(A::VariantGet {
                representation,
                case,
                field,
                value,
                ..
            }) if *representation == schema.computation && *case == tag => Some(FinishProjection {
                owner: origin(*value),
                field: *field,
            }),
            _ => None,
        };
        for tag in [2, 3] {
            let case = cases
                .iter()
                .find(|case| case.tag == tag)
                .ok_or("execution step is absent from input dispatch")?;
            let Some(A::DirectCall {
                function: callee,
                arguments,
                ..
            }) = definitions.get(&origin(case.value)).copied()
            else {
                return Err("execution step must produce its executor transition");
            };
            if *callee != function.symbol || arguments.len() != 6 {
                return Err("execution step must produce its executor transition");
            }
            if origin(arguments[0]) != origin(parameters[0])
                || origin(arguments[1]) != origin(parameters[1])
                || origin(arguments[5]) != origin(parameters[5])
            {
                return Err(
                    "execution step must preserve state, root handler and dispatch context",
                );
            }
            let Some(A::VariantNew {
                representation,
                case: queue_tag,
                fields,
                ..
            }) = definitions.get(&origin(arguments[3])).copied()
            else {
                return Err("execution step must retain its pending queue");
            };
            if *representation != queue || fields.len() != 2 {
                return Err("execution step must retain its pending queue");
            }
            if tag == 2 {
                if *queue_tag != 1 || origin(arguments[4]) != origin(parameters[4]) {
                    return Err(
                        "Bind must retain its current execution extent and prepend one callback",
                    );
                }
                bind_relation(
                    origin(parameters[2]),
                    origin(parameters[3]),
                    projection(arguments[2], 2),
                    projection(fields[0], 2),
                    origin(fields[1]),
                )?;
            } else {
                if *queue_tag != 2 || origin(fields[1]) != origin(parameters[3]) {
                    return Err("Resume must append replay to its current queue");
                }
                let Some(A::VariantNew {
                    representation,
                    case: 0,
                    fields: answer,
                    ..
                }) = definitions.get(&origin(arguments[2])).copied()
                else {
                    return Err("Resume must return its current saved answer");
                };
                if *representation != schema.computation || answer.len() != 1 {
                    return Err("Resume must return its current saved answer");
                }
                resume_answer_relation(origin(parameters[2]), projection(answer[0], 3))?;
                let replay = |value, expected| match definitions.get(&origin(value)).copied() {
                    Some(A::ProductGet {
                        representation,
                        field,
                        value,
                        ..
                    }) if *field == expected => Some((*representation, origin(*value))),
                    _ => None,
                };
                let result_schema = table
                    .control_replay_schema()
                    .ok_or("Resume has no replay schema")?
                    .result;
                let Some((representation, owner)) = replay(fields[0], 0) else {
                    return Err("Resume must use its replayed queue and activation");
                };
                if representation != result_schema
                    || replay(arguments[4], 1) != Some((representation, owner))
                {
                    return Err("Resume must use its replayed queue and activation");
                }
                if !matches!(definitions.get(&owner).copied(), Some(A::DirectCall { function: callee, .. }) if table.control_helpers.iter().any(|h| h.role() == R::CloneSequence && h.function() == *callee))
                {
                    return Err("Resume must use its replayed queue and activation");
                }
            }
        }
        Ok(())
    })();
    result.map_err(|message| {
        vec![
            BackendError::invalid_ir("P8 CC execution steps", function.span, message)
                .with_module(function.symbol.module),
        ]
    })
}

//! Bind and Resume operand identities on actual executor transitions.
use crate::BackendError;
use crate::cc::ControlHelperRole as R;
use crate::control::provenance::FinishProjection;
use crate::control::steps::{bind_relation, resume_answer_relation};
use crate::mir::value_identity::{definitions, loop_header, loop_parameters, reference_origin};
use crate::mir::{Instruction as I, Module, Terminator as T};

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else {
        return;
    };
    let table = layout.representation_table();
    let Some(helper) = table
        .control_helpers
        .iter()
        .find(|h| h.role() == R::Executor)
    else {
        return;
    };
    let Some(function) = module
        .functions
        .iter()
        .find(|f| f.symbol == helper.function())
    else {
        return;
    };
    let result = (|| {
        let schema = table
            .control_checkpoint_schema()
            .ok_or("execution step has no checked schema")?;
        let queue = table
            .control_sequence()
            .ok_or("execution step has no checked queue")?;
        let base = layout
            .repr_index(schema.computation)
            .map_err(|_| "execution step has no input layout")?;
        let bind = layout
            .variant_index(schema.computation, 2)
            .map_err(|_| "Bind has no physical case")?;
        let resume = layout
            .variant_index(schema.computation, 3)
            .map_err(|_| "Resume has no physical case")?;
        let returned = layout
            .variant_index(schema.computation, 0)
            .map_err(|_| "Return has no physical case")?;
        let cons = layout
            .variant_index(queue, 1)
            .map_err(|_| "Cons has no physical case")?;
        let append = layout
            .variant_index(queue, 2)
            .map_err(|_| "Append has no physical case")?;
        let replay = layout
            .repr_index(
                table
                    .control_replay_schema()
                    .ok_or("Resume has no replay schema")?
                    .result,
            )
            .map_err(|_| "Resume has no replay layout")?;
        let definitions = definitions(function);
        let origin = |v| reference_origin(v, &definitions);
        let parameters = loop_parameters(function);
        if parameters.len() != 6 {
            return Err("execution step has an invalid executor contract");
        }
        let states = crate::mir::branch_facts::compute(function);
        let projection = |value, ty| match definitions.get(&origin(value)).copied() {
            Some(I::StructGet {
                type_index,
                field,
                value,
                ..
            }) if *type_index == ty && *field > 0 => Some(FinishProjection {
                owner: origin(*value),
                field: field - 1,
            }),
            _ => None,
        };
        let replay_field = |value, expected| match definitions.get(&origin(value)).copied() {
            Some(I::StructGet {
                type_index,
                field,
                value,
                ..
            }) if *type_index == replay && *field == expected => Some(origin(*value)),
            _ => None,
        };
        let mut counts = [0, 0];
        for block in &function.blocks {
            let Some(tag @ (2 | 3)) = states
                .get(&block.id)
                .and_then(|s| s.get(&(base, origin(parameters[2]))))
                .copied()
            else {
                continue;
            };
            let transitions = block
                .instructions
                .iter()
                .filter_map(|i| match i {
                    I::Call {
                        function: callee,
                        arguments,
                        ..
                    } if *callee == function.symbol => Some(arguments.as_slice()),
                    _ => None,
                })
                .chain(match &block.terminator {
                    Some(T::Jump {
                        target, arguments, ..
                    }) if Some(*target) == loop_header(function) => Some(arguments.as_slice()),
                    Some(T::ReturnCall {
                        function: callee,
                        arguments,
                        ..
                    }) if *callee == function.symbol => Some(arguments.as_slice()),
                    _ => None,
                });
            for arguments in transitions {
                if arguments.len() != 6 {
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
                let Some(I::StructNew {
                    type_index,
                    arguments: pending,
                    ..
                }) = definitions.get(&origin(arguments[3])).copied()
                else {
                    return Err("execution step must retain its pending queue");
                };
                if pending.len() != 3 {
                    return Err("execution step must retain its pending queue");
                }
                if tag == 2 {
                    if *type_index != cons || origin(arguments[4]) != origin(parameters[4]) {
                        return Err(
                            "Bind must retain its current execution extent and prepend one callback",
                        );
                    }
                    bind_relation(
                        origin(parameters[2]),
                        origin(parameters[3]),
                        projection(arguments[2], bind),
                        projection(pending[1], bind),
                        origin(pending[2]),
                    )?;
                } else {
                    if *type_index != append || origin(pending[2]) != origin(parameters[3]) {
                        return Err("Resume must append replay to its current queue");
                    }
                    let Some(I::StructNew {
                        type_index,
                        arguments: answer,
                        ..
                    }) = definitions.get(&origin(arguments[2])).copied()
                    else {
                        return Err("Resume must return its current saved answer");
                    };
                    if *type_index != returned || answer.len() != 2 {
                        return Err("Resume must return its current saved answer");
                    }
                    resume_answer_relation(origin(parameters[2]), projection(answer[1], resume))?;
                    let owner = replay_field(pending[1], 0)
                        .ok_or("Resume must use its replayed queue and activation")?;
                    if replay_field(arguments[4], 1) != Some(owner)
                        || !matches!(definitions.get(&owner).copied(), Some(I::Call { function: callee, .. }) if table.control_helpers.iter().any(|h| h.role() == R::CloneSequence && h.function() == *callee))
                    {
                        return Err("Resume must use its replayed queue and activation");
                    }
                }
                counts[(tag - 2) as usize] += 1;
            }
            if matches!(
                &block.terminator,
                Some(T::Return { .. } | T::ReturnCallRef { .. })
            ) {
                return Err("execution step must produce its executor transition");
            }
        }
        if counts.contains(&0) {
            return Err("execution step must produce its executor transition");
        }
        Ok(())
    })();
    if let Err(message) = result {
        errors.push(
            BackendError::invalid_ir("P9 MIR execution steps", function.span, message)
                .with_module(function.symbol.module),
        );
    }
}

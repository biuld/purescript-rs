//! Target completion construction and finish copying after physical lowering.
use crate::BackendError;
use crate::cc::ControlHelperRole as R;
use crate::control::provenance::{FinishProjection, context_finish_relation};
use crate::mir::value_identity::{definitions, loop_header, loop_parameters, reference_origin};
use crate::mir::{Function, Instruction as I, Module, Terminator as T};
use crate::types::DefinedTypeId;

struct Schema {
    context: DefinedTypeId,
    finish: DefinedTypeId,
    append: DefinedTypeId,
}

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else { return };
    let table = layout.representation_table();
    let Some(executor) = table
        .control_helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
    else {
        return;
    };
    let Some(function) = module
        .functions
        .iter()
        .find(|function| function.symbol == executor.function())
    else {
        return;
    };
    let types = table
        .control_checkpoint_schema()
        .zip(table.control_sequence())
        .and_then(|(schema, sequence)| {
            Some((
                layout.variant_index(schema.context, 1).ok()?,
                layout.variant_index(sequence, 3).ok()?,
                layout.variant_index(sequence, 2).ok()?,
            ))
        });
    let Some((context, finish, append)) = types else {
        errors.push(BackendError::invalid_ir(
            "P9 MIR finish creation",
            function.span,
            "finish producer has no physical context or queue schema",
        ));
        return;
    };
    if let Err(message) = verify_function(
        function,
        &Schema {
            context,
            finish,
            append,
        },
    ) {
        errors.push(
            BackendError::invalid_ir("P9 MIR finish creation", function.span, message)
                .with_module(function.symbol.module),
        );
    }
}

fn verify_function(function: &Function, schema: &Schema) -> Result<(), &'static str> {
    let parameters = loop_parameters(function);
    if parameters.len() != 6 {
        return Err("finish producer has an invalid executor contract");
    }
    let definitions = definitions(function);
    let origin = |value| reference_origin(value, &definitions);
    let projection = |value, expected| match definitions.get(&origin(value)).copied() {
        Some(I::StructGet {
            type_index,
            field,
            value,
            ..
        }) if *type_index == expected && *field > 0 => Some(FinishProjection {
            owner: origin(*value),
            field: field - 1,
        }),
        _ => None,
    };
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let I::StructNew {
                type_index,
                arguments,
                destination,
                ..
            } = instruction
            else {
                continue;
            };
            if *type_index != schema.finish {
                continue;
            }
            if arguments.len() != 4
                || !matches!(
                    definitions.get(&origin(arguments[0])).copied(),
                    Some(I::Constant { value: 3, .. })
                )
            {
                return Err("finish producer has an invalid frame schema");
            }
            if projection(arguments[1], schema.context).is_some() {
                context_finish_relation(
                    parameters[5],
                    projection(arguments[1], schema.context),
                    projection(arguments[2], schema.context),
                    projection(arguments[3], schema.context),
                )?;
                let next = block.instructions[index + 1..]
                    .iter()
                    .find_map(|instruction| match instruction {
                        I::Call {
                            function: target,
                            arguments,
                            ..
                        } if *target == function.symbol => Some(arguments.as_slice()),
                        _ => None,
                    })
                    .or_else(|| match &block.terminator {
                        Some(T::Jump {
                            target, arguments, ..
                        }) if Some(*target) == loop_header(function) => Some(arguments.as_slice()),
                        Some(T::ReturnCall {
                            function: target,
                            arguments,
                            ..
                        }) if *target == function.symbol => Some(arguments.as_slice()),
                        _ => None,
                    })
                    .ok_or("target finish has no executor transition")?;
                if next.len() != 6
                    || origin(next[3]) != *destination
                    || origin(next[4]) != origin(arguments[1])
                    || projection(next[5], schema.context)
                        != Some(FinishProjection {
                            owner: parameters[5],
                            field: 4,
                        })
                {
                    return Err(
                        "target finish must enter its caller context with the saved extent",
                    );
                }
            } else {
                let extent = projection(arguments[1], schema.finish)
                    .filter(|projection| projection.field == 0)
                    .ok_or("finish copy must preserve its original extent")?;
                if projection(arguments[3], schema.finish)
                    != Some(FinishProjection {
                        owner: extent.owner,
                        field: 2,
                    })
                {
                    return Err("finish copy must preserve its original parent");
                }
                let Some(I::StructNew {
                    type_index,
                    arguments: appended,
                    ..
                }) = definitions.get(&origin(arguments[2])).copied()
                else {
                    return Err("finish copy must reassociate the original queue");
                };
                if *type_index != schema.append
                    || appended.len() != 3
                    || projection(appended[1], schema.finish)
                        != Some(FinishProjection {
                            owner: extent.owner,
                            field: 1,
                        })
                {
                    return Err("finish copy must reassociate the original queue");
                }
                let left = projection(extent.owner, schema.append)
                    .filter(|projection| projection.field == 0)
                    .ok_or("finish copy has no original append input")?;
                if left.owner != parameters[3]
                    || projection(appended[2], schema.append)
                        != Some(FinishProjection {
                            owner: left.owner,
                            field: 1,
                        })
                {
                    return Err("finish copy must retain the original append tail");
                }
            }
        }
    }
    Ok(())
}

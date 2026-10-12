//! Check source Handle entry over physical fields and executor loop transitions.
use crate::BackendError;
use crate::cc::ControlHelperRole as R;
use crate::control_provenance::{FinishProjection, context_clause_relation, context_tail_relation};
use crate::mir::value_identity::{
    definitions, fresh_iteration_allocation, loop_header, loop_parameters, reference_origin,
};
use crate::mir::{Function, Instruction as I, Module, Terminator as T};
use crate::types::DefinedTypeId;
use std::collections::HashMap;

struct Schema {
    context: DefinedTypeId,
    handle: DefinedTypeId,
    finish: DefinedTypeId,
    empty: DefinedTypeId,
    boundary: DefinedTypeId,
    status: DefinedTypeId,
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
    let schema = table
        .control_checkpoint_schema()
        .zip(table.control_replay_schema())
        .and_then(|(checkpoint, replay)| {
            Some(Schema {
                context: layout.variant_index(checkpoint.context, 1).ok()?,
                handle: layout.variant_index(checkpoint.computation, 4).ok()?,
                finish: layout.variant_index(replay.sequence, 3).ok()?,
                empty: layout.variant_index(replay.sequence, 0).ok()?,
                boundary: layout.variant_index(replay.boundary, 1).ok()?,
                status: layout.repr_index(replay.status).ok()?,
            })
        });
    let result = schema
        .ok_or("source context has no physical schema")
        .and_then(|schema| verify_function(function, &schema));
    if let Err(message) = result {
        errors.push(
            BackendError::invalid_ir("P9 MIR context entry", function.span, message)
                .with_module(function.symbol.module),
        );
    }
}

fn verify_function(function: &Function, schema: &Schema) -> Result<(), &'static str> {
    let parameters = loop_parameters(function);
    if parameters.len() != 6 {
        return Err("source context has an invalid executor contract");
    }
    let definitions = definitions(function);
    let origin = |value| reference_origin(value, &definitions);
    let constant = |value, expected| {
        matches!(definitions.get(&origin(value)).copied(),
        Some(I::Constant { value, .. }) if *value == expected)
    };
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
    let points = function
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .instructions
                .iter()
                .enumerate()
                .filter_map(move |(index, instruction)| {
                    instruction
                        .destination()
                        .map(|value| (value, (block.id, index)))
                })
        })
        .collect::<HashMap<_, _>>();
    let dominators = crate::mir::cfg::dominators(function);
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
            if *type_index != schema.context {
                continue;
            }
            if arguments.len() != 7 || !constant(arguments[0], 1) {
                return Err("source context has an invalid frame schema");
            }
            let fields = &arguments[1..];
            context_clause_relation(
                parameters[2],
                projection(fields[0], schema.handle),
                projection(fields[1], schema.handle),
            )?;
            if origin(fields[4]) != parameters[5] {
                return Err("source context must retain its current caller context");
            }
            if projection(fields[5], schema.finish).is_some() {
                context_tail_relation(
                    parameters[3],
                    projection(fields[2], schema.finish),
                    projection(fields[3], schema.finish),
                    projection(fields[5], schema.finish),
                )?;
            } else {
                if origin(fields[2]) != parameters[3] || origin(fields[3]) != parameters[4] {
                    return Err("source context must save its current queue and execution parent");
                }
                let extent = origin(fields[5]);
                let Some(I::StructNew {
                    type_index,
                    arguments: activation,
                    ..
                }) = definitions.get(&extent).copied()
                else {
                    return Err("source context must allocate a fresh child extent");
                };
                if *type_index != schema.boundary
                    || activation.len() != 3
                    || !constant(activation[0], 1)
                    || origin(activation[2]) != parameters[4]
                {
                    return Err("source context extent must retain its current execution parent");
                }
                let status = origin(activation[1]);
                let initialized = match definitions.get(&status).copied() {
                    Some(I::ArrayNew {
                        type_index,
                        elements,
                        ..
                    }) => {
                        *type_index == schema.status
                            && elements.len() == 1
                            && constant(elements[0], 0)
                    }
                    Some(I::ArrayNewFilled {
                        type_index,
                        length,
                        value,
                        ..
                    }) => {
                        *type_index == schema.status && constant(*length, 1) && constant(*value, 0)
                    }
                    _ => false,
                };
                if !initialized
                    || !fresh_iteration_allocation(
                        function,
                        status,
                        (block.id, index),
                        &points,
                        &dominators,
                    )
                    || !fresh_iteration_allocation(
                        function,
                        extent,
                        (block.id, index),
                        &points,
                        &dominators,
                    )
                {
                    return Err(
                        "source context extent needs a fresh live status cell in its iteration",
                    );
                }
            }
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
                .ok_or("source context has no executor transition")?;
            let empty = matches!(next.get(3).and_then(|value| definitions.get(&origin(*value))).copied(),
                Some(I::StructNew { type_index, arguments, .. })
                    if *type_index == schema.empty && arguments.len() == 1 && constant(arguments[0], 0));
            if next.len() != 6
                || origin(next[0]) != parameters[0]
                || origin(next[1]) != parameters[1]
                || projection(next[2], schema.handle)
                    != Some(FinishProjection {
                        owner: parameters[2],
                        field: 2,
                    })
                || !empty
                || origin(next[4]) != origin(fields[5])
                || origin(next[5]) != *destination
            {
                return Err(
                    "source context transition must enter its Handle body with an empty queue and its own extent",
                );
            }
        }
    }
    Ok(())
}

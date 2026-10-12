//! Check replay lifecycle allocations and transitions after loop conversion.
use crate::BackendError;
use crate::cc::ControlHelperRole as R;
use crate::control_provenance::{
    FinishProjection, replay_activation_relation, saved_finish_relation,
};
use crate::mir::value_identity::{definitions, loop_header, loop_parameters, reference_origin};
use crate::mir::{Function, Instruction as I, Module, Terminator as T};
use crate::types::{DefinedTypeId, ValueId};
use std::collections::{HashMap, HashSet};

type Point = (crate::mir::BlockId, usize);

struct Schema {
    finish: DefinedTypeId,
    cons: DefinedTypeId,
    boundary: DefinedTypeId,
    status: DefinedTypeId,
    result: DefinedTypeId,
}

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else { return };
    let table = layout.representation_table();
    let functions = module
        .functions
        .iter()
        .filter_map(|function| {
            let role = table
                .control_helpers
                .iter()
                .find(|helper| helper.function() == function.symbol)?
                .role();
            matches!(role, R::CloneSequence | R::RebuildSequence).then_some((function, role))
        })
        .collect::<Vec<_>>();
    if functions.is_empty() {
        return;
    }
    let Some(requirements) = table.control_replay_schema() else {
        errors.push(BackendError::invalid_ir(
            "P9 MIR control replay",
            module.span,
            "replay has no checked lifecycle schema",
        ));
        return;
    };
    let types = (
        layout.variant_index(requirements.sequence, 3),
        layout.variant_index(requirements.sequence, 1),
        layout.variant_index(requirements.boundary, 1),
        layout.repr_index(requirements.status),
        layout.repr_index(requirements.result),
    );
    let (Ok(finish), Ok(cons), Ok(boundary), Ok(status), Ok(result)) = types else {
        errors.push(BackendError::invalid_ir(
            "P9 MIR control replay",
            module.span,
            "replay lifecycle schema has no physical layout",
        ));
        return;
    };
    let schema = Schema {
        finish,
        cons,
        boundary,
        status,
        result,
    };
    let read_only = crate::mir::opt::effects::read_only_functions(module);
    for (function, role) in functions {
        if let Err(message) = verify_function(function, role, table, &schema, &read_only) {
            errors.push(
                BackendError::invalid_ir("P9 MIR control replay", function.span, message)
                    .with_module(function.symbol.module),
            );
        }
    }
}

fn verify_function(
    function: &Function,
    role: R,
    table: &crate::cc::RepresentationTable,
    schema: &Schema,
    read_only: &HashSet<psrs_hir::SymbolId>,
) -> Result<(), &'static str> {
    let parameters = loop_parameters(function);
    if parameters.len() != if role == R::CloneSequence { 5 } else { 4 } {
        return Err("replay has an invalid parameter contract");
    }
    let definitions = definitions(function);
    let origin = |value| reference_origin(value, &definitions);
    let constant = |value, expected| matches!(definitions.get(&origin(value)).copied(), Some(I::Constant { value, .. }) if *value == expected);
    let projection = |value, expected_type| match definitions.get(&origin(value)).copied() {
        Some(I::StructGet {
            type_index,
            field,
            value,
            ..
        }) if *type_index == expected_type && *field > 0 => Some(FinishProjection {
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
                        .map(|destination| (destination, (block.id, index)))
                })
        })
        .collect::<HashMap<_, _>>();
    let dominators = crate::mir::cfg::dominators(function);
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            let read_only_call = matches!(instruction,
                I::Call { function, .. } | I::CallVoid { function, .. } if read_only.contains(function));
            if crate::mir::opt::effects::may_invoke(instruction)
                && !read_only_call
                && !matches!(instruction, I::Call { contract: Some(id), .. }
                    if table.control_helpers.get(id.0 as usize).is_some_and(|helper|
                        matches!(helper.role(), R::Check | R::CloneSequence | R::RebuildSequence)))
            {
                return Err("replay traversal cannot invoke callbacks or mutate lifecycle cells");
            }
            match instruction {
                I::ClosureNew { .. } => {
                    return Err(
                        "replay traversal cannot invoke callbacks or mutate lifecycle cells",
                    );
                }
                I::StructNew {
                    type_index,
                    arguments,
                    destination,
                    ..
                } if *type_index == schema.cons => {
                    let accumulator = parameters[if role == R::CloneSequence { 2 } else { 1 }];
                    if arguments.len() != 3
                        || !constant(arguments[0], 1)
                        || projection(arguments[1], schema.cons)
                            != Some(FinishProjection {
                                owner: parameters[0],
                                field: 0,
                            })
                        || origin(arguments[2]) != accumulator
                    {
                        return Err(
                            "replay must share the original callback and retain its accumulator",
                        );
                    }
                    if role == R::RebuildSequence {
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
                                }) if Some(*target) == loop_header(function) => {
                                    Some(arguments.as_slice())
                                }
                                Some(T::ReturnCall {
                                    function: target,
                                    arguments,
                                    ..
                                }) if *target == function.symbol => Some(arguments.as_slice()),
                                _ => None,
                            })
                            .ok_or("callback rebuild has no transition")?;
                        if next.len() != 4
                            || projection(next[0], schema.cons)
                                != Some(FinishProjection {
                                    owner: parameters[0],
                                    field: 1,
                                })
                            || origin(next[1]) != *destination
                            || origin(next[2]) != parameters[2]
                            || origin(next[3]) != parameters[3]
                        {
                            return Err(
                                "callback rebuild must preserve its active extent and parent",
                            );
                        }
                    }
                }
                I::StructNew {
                    type_index,
                    arguments,
                    destination,
                    ..
                } if *type_index == schema.finish => {
                    if arguments.len() != 4 || !constant(arguments[0], 3) {
                        return Err("replay finish has an invalid schema");
                    }
                    if role == R::CloneSequence {
                        saved_finish_relation(
                            parameters[0],
                            projection(arguments[1], schema.finish),
                        )?;
                        if origin(arguments[2]) != parameters[2]
                            || origin(arguments[3]) != parameters[4]
                        {
                            return Err(
                                "replay traversal must retain its reverse accumulator and parent",
                            );
                        }
                        let checked = function.blocks.iter().any(|source| source.instructions.iter().enumerate().any(|(position, instruction)| {
                            precedes((source.id, position), (block.id, index), &dominators)
                                && matches!(instruction, I::Call { contract: Some(id), arguments: checked, .. }
                                    if table.control_helpers.get(id.0 as usize).is_some_and(|helper| helper.role() == R::Check)
                                        && checked.len() == 1 && origin(checked[0]) == origin(arguments[1]))
                        }));
                        if !checked {
                            return Err(
                                "replay must check the saved finish extent before copying it",
                            );
                        }
                    } else {
                        replay_activation_relation(
                            parameters[1],
                            parameters[2],
                            origin(arguments[2]),
                            origin(arguments[3]),
                        )?;
                        let extent = origin(arguments[1]);
                        let Some(I::StructNew {
                            type_index,
                            arguments: activation,
                            ..
                        }) = definitions.get(&extent).copied()
                        else {
                            return Err("replay must create a fresh activation extent");
                        };
                        if *type_index != schema.boundary
                            || activation.len() != 3
                            || !constant(activation[0], 1)
                            || origin(activation[2]) != parameters[3]
                        {
                            return Err("replay activation must use its current parent");
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
                                *type_index == schema.status
                                    && constant(*length, 1)
                                    && constant(*value, 0)
                            }
                            _ => false,
                        };
                        let point = (block.id, index);
                        if !initialized
                            || !fresh_iteration_allocation(
                                function,
                                status,
                                point,
                                &points,
                                &dominators,
                            )
                            || !fresh_iteration_allocation(
                                function,
                                extent,
                                point,
                                &points,
                                &dominators,
                            )
                        {
                            return Err(
                                "replay activation needs a fresh live status cell in its iteration",
                            );
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
                                }) if Some(*target) == loop_header(function) => {
                                    Some(arguments.as_slice())
                                }
                                Some(T::ReturnCall {
                                    function: target,
                                    arguments,
                                    ..
                                }) if *target == function.symbol => Some(arguments.as_slice()),
                                _ => None,
                            })
                            .ok_or("replay activation has no rebuild transition")?;
                        if next.len() != 4
                            || origin(next[1]) != *destination
                            || origin(next[2]) != extent
                            || origin(next[3]) != extent
                        {
                            return Err(
                                "replay must advance its queue, active extent and parent together",
                            );
                        }
                    }
                }
                I::StructNew {
                    type_index,
                    arguments,
                    ..
                } if role == R::RebuildSequence && *type_index == schema.result => {
                    if arguments.len() != 2
                        || origin(arguments[0]) != parameters[1]
                        || origin(arguments[1]) != parameters[2]
                    {
                        return Err(
                            "replay result must return its accumulated queue and active extent",
                        );
                    }
                }
                I::ArraySet { .. } => {
                    return Err("replay must not mutate saved or fresh lifecycle cells");
                }
                _ => {}
            }
        }
        if role == R::CloneSequence {
            let calls = block
                .instructions
                .iter()
                .filter_map(|instruction| match instruction {
                    I::Call {
                        contract: Some(id),
                        arguments,
                        ..
                    } => Some((*id, arguments.as_slice())),
                    _ => None,
                })
                .chain(
                    block
                        .terminator
                        .iter()
                        .filter_map(|terminator| match terminator {
                            T::ReturnCall {
                                contract: Some(id),
                                arguments,
                                ..
                            } => Some((*id, arguments.as_slice())),
                            _ => None,
                        }),
                );
            for (id, arguments) in calls {
                let valid = match table
                    .control_helpers
                    .get(id.0 as usize)
                    .map(|helper| helper.role())
                {
                    Some(R::CloneSequence) => {
                        arguments.len() == 5
                            && origin(arguments[3]) == parameters[3]
                            && origin(arguments[4]) == parameters[4]
                    }
                    Some(R::RebuildSequence) => {
                        arguments.len() == 4
                            && origin(arguments[0]) == parameters[2]
                            && origin(arguments[1]) == parameters[1]
                            && origin(arguments[2]) == parameters[3]
                            && origin(arguments[3]) == parameters[4]
                    }
                    _ => true,
                };
                if !valid {
                    return Err(
                        "clone traversal must preserve its accumulators, active extent and parent",
                    );
                }
            }
            if let Some(T::Jump {
                target, arguments, ..
            }) = &block.terminator
                && block.id != function.entry
                && Some(*target) == loop_header(function)
                && (arguments.len() != 5
                    || origin(arguments[3]) != parameters[3]
                    || origin(arguments[4]) != parameters[4])
            {
                return Err(
                    "clone traversal must preserve its accumulators, active extent and parent",
                );
            }
        }
        match &block.terminator {
            Some(T::ReturnCallRef { .. }) => {
                return Err("replay traversal cannot invoke callbacks or mutate lifecycle cells");
            }
            Some(T::ReturnCall {
                function: target,
                contract,
                ..
            }) if contract
                .and_then(|id| table.control_helpers.get(id.0 as usize))
                .is_none_or(|helper| {
                    !matches!(
                        helper.role(),
                        R::Check | R::CloneSequence | R::RebuildSequence
                    )
                }) =>
            {
                return Err("replay traversal cannot invoke callbacks or mutate lifecycle cells");
            }
            _ => {}
        }
    }
    Ok(())
}

fn precedes(
    source: Point,
    target: Point,
    dominators: &HashMap<crate::mir::BlockId, HashSet<crate::mir::BlockId>>,
) -> bool {
    if source.0 == target.0 {
        source.1 < target.1
    } else {
        dominators
            .get(&target.0)
            .is_some_and(|set| set.contains(&source.0))
    }
}

fn fresh_iteration_allocation(
    function: &Function,
    allocation: ValueId,
    consumer: Point,
    points: &HashMap<ValueId, Point>,
    dominators: &HashMap<crate::mir::BlockId, HashSet<crate::mir::BlockId>>,
) -> bool {
    let Some(point) = points.get(&allocation).copied() else {
        return false;
    };
    precedes(point, consumer, dominators)
        && loop_header(function).is_none_or(|header| {
            dominators
                .get(&point.0)
                .is_some_and(|set| set.contains(&header))
        })
}

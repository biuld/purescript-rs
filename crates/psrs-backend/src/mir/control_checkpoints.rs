//! Preserve checkpoint ownership after physical projection and loop conversion.
use super::{Function, Instruction as I, Module};
use crate::BackendError;
use crate::cc::ControlHelperRole as R;
use crate::control_provenance::{
    CaptureProjection, checkpoint_capture_relation, dispatch_checkpoint_relation,
};
use crate::types::{DefinedTypeId, ValueId};
use std::collections::HashMap;

use super::value_identity::{
    Definitions, definitions, loop_parameters, reference_origin as origin,
};

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else { return };
    let table = layout.representation_table();
    let Some(raw) = &table.control_resumption else {
        return;
    };
    let Some(executor) = table
        .control_helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
    else {
        return;
    };
    if !module
        .functions
        .iter()
        .any(|function| function.symbol == raw.function() || function.symbol == executor.function())
    {
        return;
    }
    let Some(schema) = table.control_checkpoint_schema() else {
        errors.push(BackendError::invalid_ir(
            "P9 MIR control checkpoint",
            module.span,
            "raw resumption has no checkpoint schema",
        ));
        return;
    };
    let types = (
        layout.variant_index(schema.computation, 3),
        layout.variant_index(schema.context, 0),
        layout.variant_index(schema.context, 1),
        layout.signature_index(raw.signature()),
        table
            .control_sequence()
            .and_then(|sequence| layout.variant_index(sequence, 0).ok()),
    );
    let (Ok(resume), Ok(root), Ok(scope), Ok(callable), Some(empty)) = types else {
        errors.push(BackendError::invalid_ir(
            "P9 MIR control checkpoint",
            module.span,
            "raw resumption schema has no physical layout",
        ));
        return;
    };
    for function in &module.functions {
        if let Err(message) = verify_function(
            function,
            table,
            raw,
            executor.function(),
            resume,
            root,
            scope,
            callable,
            empty,
        ) {
            errors.push(
                BackendError::invalid_ir("P9 MIR control checkpoint", function.span, message)
                    .with_module(function.symbol.module),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn verify_function(
    function: &Function,
    table: &crate::cc::RepresentationTable,
    raw: &crate::cc::ControlResumptionContract,
    executor: psrs_hir::SymbolId,
    resume: DefinedTypeId,
    root: DefinedTypeId,
    scope: DefinedTypeId,
    callable: DefinedTypeId,
    empty: DefinedTypeId,
) -> Result<(), &'static str> {
    let definitions = definitions(function);
    let origin = |value| origin(value, &definitions);
    let dominators = super::cfg::dominators(function);
    let parameters = loop_parameters(function);
    if function.symbol == executor && parameters.len() != 6 {
        return Err("control executor has an invalid parameter contract");
    }
    if function.symbol == raw.function() {
        if parameters.len() != 2 {
            return Err("raw resumption has an invalid parameter contract");
        }
        let result = origin(function.result);
        let Some(I::StructNew {
            type_index,
            arguments,
            ..
        }) = definitions.get(&result).copied()
        else {
            return Err("raw resumption must return its saved Resume frame");
        };
        if *type_index != resume
            || arguments.len() != 4
            || origin(arguments[1]) != parameters[1]
            || !matches!(
                definitions.get(&origin(arguments[0])).copied(),
                Some(I::Constant { value: 3, .. })
            )
        {
            return Err("raw resumption frame has an invalid answer or schema");
        }
        checkpoint_capture_relation(
            parameters[0],
            capture(arguments[2], &definitions),
            capture(arguments[3], &definitions),
        )?;
        let point = function
            .blocks
            .iter()
            .find_map(|block| {
                block
                    .instructions
                    .iter()
                    .position(|instruction| instruction.destination() == Some(result))
                    .map(|index| (block.id, index))
            })
            .ok_or("raw resumption frame has no checked construction sequence")?;
        require_check(
            function,
            table,
            point,
            origin(arguments[3]),
            &definitions,
            &dominators,
        )?;
    }
    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            match instruction {
                I::ClosureNew {
                    function: target,
                    captures,
                    type_index,
                    ..
                } if *target == raw.function() => {
                    if function.symbol != executor
                        || parameters.len() != 6
                        || captures.len() != 2
                        || *type_index != callable
                        || origin(captures[0]) != parameters[3]
                    {
                        return Err(
                            "raw resumption factory must capture the executor's current queue",
                        );
                    }
                    let projection = match definitions.get(&origin(captures[1])).copied() {
                        Some(I::StructGet {
                            type_index,
                            field,
                            value,
                            ..
                        }) if *field > 0 && (*type_index == root || *type_index == scope) => {
                            Some((origin(*value), u32::from(*type_index == scope), field - 1))
                        }
                        _ => None,
                    };
                    dispatch_checkpoint_relation(parameters[5], projection)?;
                }
                I::Call {
                    contract: Some(contract),
                    arguments,
                    ..
                } if function.symbol == executor
                    && table
                        .control_helpers
                        .get(contract.0 as usize)
                        .is_some_and(|helper| helper.role() == R::CloneSequence) =>
                {
                    if arguments.len() != 5 || origin(arguments[3]) != parameters[4] {
                        return Err("replay must start with its current execution extent");
                    }
                    for initial in &arguments[1..3] {
                        if !matches!(definitions.get(&origin(*initial)).copied(), Some(I::StructNew { type_index, arguments, .. })
                            if *type_index == empty && arguments.len() == 1
                                && matches!(definitions.get(&origin(arguments[0])).copied(), Some(I::Constant { value: 0, .. })))
                        {
                            return Err("replay must start with empty traversal accumulators");
                        }
                    }
                    let parents =
                        super::value_identity::incoming_values(function, origin(arguments[4]))
                            .ok_or("replay parent must select its current dispatch context")?;
                    let mut seen = std::collections::HashSet::new();
                    for parent in parents {
                        let projection = match definitions.get(&origin(parent)).copied() {
                            Some(I::StructGet {
                                type_index,
                                field,
                                value,
                                ..
                            }) if *field > 0 && (*type_index == root || *type_index == scope) => {
                                let case = u32::from(*type_index == scope);
                                seen.insert(case);
                                Some((origin(*value), case, field - 1))
                            }
                            _ => None,
                        };
                        dispatch_checkpoint_relation(parameters[5], projection)?;
                    }
                    if seen != std::collections::HashSet::from([0, 1]) {
                        return Err("replay parent must cover root and scoped dispatch contexts");
                    }
                    let saved = arguments.first().ok_or("replay has no saved queue")?;
                    let Some(I::StructGet {
                        type_index,
                        field: 2,
                        value,
                        ..
                    }) = definitions.get(&origin(*saved)).copied()
                    else {
                        return Err("replay must use the Resume frame's saved queue");
                    };
                    if *type_index != resume || origin(*value) != parameters[2] {
                        return Err("replay must use the current Resume frame");
                    }
                    let checkpoint = definitions
                        .iter()
                        .find_map(|(destination, instruction)| match instruction {
                            I::StructGet {
                                type_index,
                                field: 3,
                                value: owner,
                                ..
                            } if *type_index == resume && origin(*owner) == origin(*value) => {
                                Some(origin(*destination))
                            }
                            _ => None,
                        })
                        .ok_or("replay has no current Resume checkpoint")?;
                    require_check(
                        function,
                        table,
                        (block.id, index),
                        checkpoint,
                        &definitions,
                        &dominators,
                    )?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn capture(value: ValueId, definitions: &Definitions<'_>) -> Option<CaptureProjection> {
    match definitions.get(&origin(value, definitions)).copied()? {
        I::ClosureGetCapture { closure, index, .. } => Some(CaptureProjection {
            receiver: origin(*closure, definitions),
            slot: *index,
        }),
        _ => None,
    }
}

fn require_check(
    function: &Function,
    table: &crate::cc::RepresentationTable,
    point: (super::BlockId, usize),
    checkpoint: ValueId,
    definitions: &Definitions<'_>,
    dominators: &HashMap<super::BlockId, std::collections::HashSet<super::BlockId>>,
) -> Result<(), &'static str> {
    let checked = function.blocks.iter().any(|block| {
        let prefix = if block.id == point.0 { &block.instructions[..point.1] }
            else if dominators.get(&point.0).is_some_and(|set| set.contains(&block.id)) { block.instructions.as_slice() }
            else { return false };
        prefix.iter().any(|instruction| matches!(instruction,
            I::Call { contract: Some(contract), arguments, .. }
                if table.control_helpers.get(contract.0 as usize).is_some_and(|helper| helper.role() == R::Check)
                    && arguments.len() == 1 && origin(arguments[0], definitions) == checkpoint))
    });
    if checked {
        Ok(())
    } else {
        Err("resumption must check its saved checkpoint before construction or replay")
    }
}

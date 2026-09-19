//! Per-function MIR verification: block structure and SSA single assignment.
//! Instruction and terminator type checks live in `super::instruction`.

use super::Signature;
use super::instruction::verify_instruction;
use super::util::{mir_error, require_value, value_type};
use crate::BackendError;
use crate::mir::{BasicBlock, BlockId, Function, Terminator, ValueId, ValueType};
use crate::types::DefinedType;
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

pub(super) fn verify_function(
    function: &Function,
    signatures: &HashMap<SymbolId, Option<Signature>>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    if blocks.len() != function.blocks.len() || !blocks.contains_key(&function.entry) {
        return Err(mir_error(
            function.span,
            "MIR block IDs are duplicated or entry is missing",
        ));
    }
    let mut definitions = HashMap::<ValueId, ValueType>::new();
    for value in &function.values {
        if definitions.insert(value.id, value.ty).is_some() {
            return Err(mir_error(function.span, "MIR value IDs are duplicated"));
        }
    }
    let mut block_ids = HashSet::new();
    for block in &function.blocks {
        for parameter in &block.parameters {
            if !block_ids.insert((block.id, *parameter)) {
                return Err(mir_error(
                    function.span,
                    "MIR block parameter is duplicated",
                ));
            }
            if !definitions.contains_key(parameter) {
                return Err(mir_error(
                    function.span,
                    "MIR block parameter has no value type",
                ));
            }
        }
        if block.terminator.is_none() {
            return Err(mir_error(
                function.span,
                "MIR basic block has no terminator",
            ));
        }
    }
    // MIR is SSA: every value is defined exactly once, by the function
    // parameters, a block parameter, or one instruction.
    let mut defined_values = HashSet::new();
    for parameter in &function.parameters {
        if !definitions.contains_key(parameter) {
            return Err(mir_error(
                function.span,
                "MIR function parameter has no value type",
            ));
        }
        defined_values.insert(*parameter);
    }
    for block in &function.blocks {
        for parameter in &block.parameters {
            defined_values.insert(*parameter);
        }
    }
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Some(destination) = instruction.destination() {
                if !definitions.contains_key(&destination) {
                    return Err(mir_error(
                        instruction.span(),
                        "MIR instruction destination has no value type",
                    ));
                }
                if !defined_values.insert(destination) {
                    return Err(mir_error(
                        instruction.span(),
                        "MIR value is defined more than once",
                    ));
                }
            }
            for operand in instruction.operands() {
                if !definitions.contains_key(&operand) {
                    return Err(mir_error(
                        instruction.span(),
                        "MIR instruction uses an unknown value",
                    ));
                }
            }
            verify_instruction(function, instruction, &definitions, signatures, defined)?;
        }
        let terminator = block.terminator.as_ref().expect("checked above");
        verify_terminator(function, terminator, &blocks, &definitions)?;
    }
    Ok(())
}

fn verify_terminator(
    function: &Function,
    terminator: &Terminator,
    blocks: &HashMap<BlockId, &BasicBlock>,
    definitions: &HashMap<ValueId, ValueType>,
) -> Result<(), Vec<BackendError>> {
    match terminator {
        Terminator::Return { value, span } => {
            if require_value(definitions, *value, *span)? != function.result_type {
                return Err(mir_error(*span, "MIR return value has the wrong type"));
            }
        }
        Terminator::Jump {
            target,
            arguments,
            span,
        } => {
            let Some(target) = blocks.get(target) else {
                return Err(mir_error(*span, "MIR jump target does not exist"));
            };
            if target.parameters.len() != arguments.len() {
                return Err(mir_error(
                    *span,
                    "MIR jump argument count differs from target parameters",
                ));
            }
            for (argument, parameter) in arguments.iter().zip(&target.parameters) {
                let expected = value_type(function, *parameter)
                    .ok_or_else(|| mir_error(*span, "MIR block parameter has no type"))?;
                if require_value(definitions, *argument, *span)? != expected {
                    return Err(mir_error(*span, "MIR jump argument has the wrong type"));
                }
            }
        }
        Terminator::Branch {
            condition,
            then_block,
            else_block,
            merge_block,
            span,
        } => {
            if require_value(definitions, *condition, *span)? != ValueType::Boolean {
                return Err(mir_error(*span, "MIR branch condition is not Boolean"));
            }
            if !blocks.contains_key(then_block) || !blocks.contains_key(else_block) {
                return Err(mir_error(*span, "MIR branch target does not exist"));
            }
            if blocks
                .get(merge_block)
                .is_none_or(|block| block.parameters.len() != 1)
            {
                return Err(mir_error(
                    *span,
                    "MIR branch merge must have one result parameter",
                ));
            }
        }
    }
    Ok(())
}

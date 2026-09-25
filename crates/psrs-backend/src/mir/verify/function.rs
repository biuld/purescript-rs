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
    // MIR is SSA: every value is defined exactly once, by a function
    // parameter, a block parameter, or one instruction. Keep the owning block
    // so operand checks can enforce dominance rather than only checking that a
    // value appears somewhere in the function.
    let mut defined_values = HashSet::new();
    let mut owners = HashMap::<ValueId, Option<BlockId>>::new();
    let mut function_parameters = HashSet::new();
    for parameter in &function.parameters {
        if !definitions.contains_key(parameter) {
            return Err(mir_error(
                function.span,
                "MIR function parameter has no value type",
            ));
        }
        if !defined_values.insert(*parameter) {
            return Err(mir_error(
                function.span,
                "MIR value is defined more than once",
            ));
        }
        function_parameters.insert(*parameter);
        owners.insert(*parameter, None);
    }
    for block in &function.blocks {
        for parameter in &block.parameters {
            if !defined_values.insert(*parameter) {
                return Err(mir_error(
                    function.span,
                    "MIR value is defined more than once",
                ));
            }
            owners.insert(*parameter, Some(block.id));
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
                owners.insert(destination, Some(block.id));
            }
        }
    }

    let dominators = compute_dominators(function.entry, &blocks);
    for block in &function.blocks {
        let mut available = block.parameters.iter().copied().collect::<HashSet<_>>();
        for instruction in &block.instructions {
            for operand in instruction.operands() {
                if !definitions.contains_key(&operand) {
                    return Err(mir_error(
                        instruction.span(),
                        "MIR instruction uses an unknown value",
                    ));
                }
                if !value_available(
                    operand,
                    block.id,
                    &available,
                    &function_parameters,
                    &owners,
                    &dominators,
                ) {
                    return Err(mir_error(
                        instruction.span(),
                        "MIR instruction uses a value before its definition or outside its dominance scope",
                    ));
                }
            }
            verify_instruction(function, instruction, &definitions, signatures, defined)?;
            if let Some(destination) = instruction.destination() {
                available.insert(destination);
            }
        }
        let terminator = block.terminator.as_ref().expect("checked above");
        for operand in terminator_operands(terminator) {
            if !definitions.contains_key(&operand) {
                return Err(mir_error(
                    terminator_span(terminator),
                    "MIR terminator uses an unknown value",
                ));
            }
            if !value_available(
                operand,
                block.id,
                &available,
                &function_parameters,
                &owners,
                &dominators,
            ) {
                return Err(mir_error(
                    terminator_span(terminator),
                    "MIR terminator uses a value before its definition or outside its dominance scope",
                ));
            }
        }
        verify_terminator(function, terminator, &blocks, &definitions)?;
    }
    super::array_map::verify_array_maps(function)?;
    super::array_map::verify_conversion_helpers(function, defined)?;
    Ok(())
}

fn value_available(
    value: ValueId,
    block: BlockId,
    local: &HashSet<ValueId>,
    function_parameters: &HashSet<ValueId>,
    owners: &HashMap<ValueId, Option<BlockId>>,
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
) -> bool {
    if local.contains(&value) || function_parameters.contains(&value) {
        return true;
    }
    let Some(Some(owner)) = owners.get(&value) else {
        return false;
    };
    *owner != block
        && dominators
            .get(&block)
            .is_some_and(|set| set.contains(owner))
}

fn compute_dominators(
    entry: BlockId,
    blocks: &HashMap<BlockId, &BasicBlock>,
) -> HashMap<BlockId, HashSet<BlockId>> {
    let all = blocks.keys().copied().collect::<HashSet<_>>();
    let mut predecessors = blocks
        .keys()
        .copied()
        .map(|id| (id, HashSet::new()))
        .collect::<HashMap<BlockId, HashSet<BlockId>>>();
    for block in blocks.values() {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        for successor in terminator_successors(terminator) {
            if let Some(preds) = predecessors.get_mut(&successor) {
                preds.insert(block.id);
            }
        }
    }

    let mut dominators = blocks
        .keys()
        .copied()
        .map(|id| {
            let initial = if id == entry {
                HashSet::from([entry])
            } else {
                all.clone()
            };
            (id, initial)
        })
        .collect::<HashMap<_, _>>();
    loop {
        let mut changed = false;
        for id in blocks.keys().copied().filter(|id| *id != entry) {
            let mut next = all.clone();
            if let Some(preds) = predecessors.get(&id) {
                if preds.is_empty() {
                    next.clear();
                } else {
                    for pred in preds {
                        if let Some(pred_dominators) = dominators.get(pred) {
                            next.retain(|candidate| pred_dominators.contains(candidate));
                        }
                    }
                }
            }
            next.insert(id);
            if dominators.get(&id) != Some(&next) {
                dominators.insert(id, next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    dominators
}

fn terminator_successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Return { .. } => Vec::new(),
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        Terminator::Switch { cases, default, .. } => cases
            .iter()
            .map(|(_, block)| *block)
            .chain(std::iter::once(*default))
            .collect(),
    }
}

fn terminator_operands(terminator: &Terminator) -> Vec<ValueId> {
    match terminator {
        Terminator::Return { value, .. } => vec![*value],
        Terminator::Jump { arguments, .. } => arguments.clone(),
        Terminator::Branch { condition, .. } => vec![*condition],
        Terminator::Switch { value, .. } => vec![*value],
    }
}

fn terminator_span(terminator: &Terminator) -> psrs_span::TextRange {
    match terminator {
        Terminator::Return { span, .. }
        | Terminator::Jump { span, .. }
        | Terminator::Branch { span, .. }
        | Terminator::Switch { span, .. } => *span,
    }
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
            span,
        } => {
            if require_value(definitions, *condition, *span)? != ValueType::Boolean {
                return Err(mir_error(*span, "MIR branch condition is not Boolean"));
            }
            if !blocks.contains_key(then_block) || !blocks.contains_key(else_block) {
                return Err(mir_error(*span, "MIR branch target does not exist"));
            }
            // `Branch` carries no arguments, so a target with parameters could
            // never receive them. This is the arity rule the structurer relies
            // on when it rewrites a constant branch into a parameterless jump.
            for target in [then_block, else_block] {
                if blocks
                    .get(target)
                    .is_some_and(|block| !block.parameters.is_empty())
                {
                    return Err(mir_error(
                        *span,
                        "MIR branch targets cannot have block parameters",
                    ));
                }
            }
        }
        Terminator::Switch {
            value,
            cases,
            default,
            span,
        } => {
            if require_value(definitions, *value, *span)? != ValueType::I32 {
                return Err(mir_error(*span, "MIR switch selector is not i32"));
            }
            if cases
                .iter()
                .map(|(case, _)| *case)
                .collect::<HashSet<_>>()
                .len()
                != cases.len()
            {
                return Err(mir_error(*span, "MIR switch case values are not unique"));
            }
            for target in cases
                .iter()
                .map(|(_, target)| target)
                .chain(std::iter::once(default))
            {
                let Some(block) = blocks.get(target) else {
                    return Err(mir_error(*span, "MIR switch target does not exist"));
                };
                if !block.parameters.is_empty() {
                    return Err(mir_error(
                        *span,
                        "MIR switch targets cannot have block parameters",
                    ));
                }
            }
        }
    }
    Ok(())
}

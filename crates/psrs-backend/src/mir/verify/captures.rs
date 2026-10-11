//! Check closure slots against the local code target before eqref storage erases
//! their types. Surviving projections define a sparse contract after optimization.

use super::util::{mir_error, value_type};
use crate::BackendError;
use crate::mir::{Function, Instruction, Module, ValueId, ValueType};
use crate::types::DefinedType;
use std::collections::{HashMap, HashSet};

pub(super) fn verify(module: &Module, defined: &[&DefinedType], errors: &mut Vec<BackendError>) {
    let contracts = module
        .functions
        .iter()
        .map(|function| {
            let receivers = receiver_aliases(function);
            let mut slots = HashMap::new();
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                if let Instruction::ClosureGetCapture {
                    destination,
                    closure,
                    index,
                    span,
                    ..
                } = instruction
                    && receivers.contains(closure)
                    && let Some(ty) = value_type(function, *destination)
                    && slots
                        .insert(*index, ty)
                        .is_some_and(|previous| previous != ty)
                {
                    errors.extend(
                        mir_error(
                            *span,
                            "MIR closure capture slot has incompatible projection types",
                        )
                        .into_iter()
                        .map(|error| error.with_module(function.symbol.module)),
                    );
                }
            }
            (function.symbol, slots)
        })
        .collect::<HashMap<_, _>>();

    for function in &module.functions {
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            let Instruction::ClosureNew {
                function: target,
                captures,
                span,
                ..
            } = instruction
            else {
                continue;
            };
            // Imported code has no local body from which to derive a contract.
            // Its ABI remains a trusted external boundary.
            let Some(slots) = contracts.get(target) else {
                continue;
            };
            for (index, expected) in slots {
                let actual = captures
                    .get(*index as usize)
                    .and_then(|value| value_type(function, *value));
                if actual.is_none_or(|actual| !compatible(actual, *expected, defined)) {
                    errors.extend(
                        mir_error(
                            *span,
                            "MIR closure captures do not match target projection types",
                        )
                        .into_iter()
                        .map(|error| error.with_module(function.symbol.module)),
                    );
                }
            }
        }
    }
}

fn receiver_aliases(function: &Function) -> HashSet<ValueId> {
    let mut aliases = function
        .parameters
        .first()
        .copied()
        .into_iter()
        .collect::<HashSet<_>>();
    loop {
        let mut changed = false;
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let Instruction::Copy {
                destination, value, ..
            }
            | Instruction::RefCast {
                destination, value, ..
            } = instruction
                && aliases.contains(value)
            {
                changed |= aliases.insert(*destination);
            }
        }
        if !changed {
            return aliases;
        }
    }
}

fn compatible(actual: ValueType, expected: ValueType, defined: &[&DefinedType]) -> bool {
    if actual == expected {
        return true;
    }
    match (actual, expected) {
        (ValueType::Ref(actual), ValueType::Ref(expected)) => {
            (!actual.nullable || expected.nullable)
                && super::subtype::heap_subtype(actual.heap, expected.heap, defined)
        }
        _ => false,
    }
}

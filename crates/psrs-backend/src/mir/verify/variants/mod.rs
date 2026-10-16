//! Nominal variant constructors must initialize the tag of their planned case.
use super::util::mir_error;
use crate::BackendError;
use crate::mir::value_identity::{definitions, integer_constant};
use crate::mir::{Instruction, Module};
use std::collections::{HashMap, HashSet};

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else {
        // Standalone structural MIR carries no source nominal layout contract.
        return;
    };
    let tags = layout.variant_tags().collect::<HashMap<_, _>>();
    let bases = layout.variant_bases().collect::<HashSet<_>>();
    let types = super::defined_types_list(module);
    for index in tags.keys().chain(&bases) {
        let valid = types.get(index.0 as usize).is_some_and(|ty| {
            matches!(&ty.composite, crate::types::CompositeType::Struct(fields)
                if fields.first().is_some_and(|field| field.storage == crate::types::StorageType::I32 && !field.mutable))
        });
        if !valid {
            errors.extend(mir_error(
                module.span,
                "MIR nominal variant tag must remain immutable i32",
            ));
        }
    }
    for function in &module.functions {
        let definitions = definitions(function);
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            let Instruction::StructNew {
                type_index,
                arguments,
                span,
                ..
            } = instruction
            else {
                continue;
            };
            if bases.contains(type_index) {
                errors.extend(mir_error(
                    *span,
                    "MIR nominal variant must be constructed at a planned case type",
                ));
                continue;
            }
            let Some(expected) = tags.get(type_index) else {
                continue;
            };
            let actual = arguments
                .first()
                .and_then(|value| integer_constant(*value, &definitions));
            if actual != Some(*expected) {
                errors.extend(
                    mir_error(
                        *span,
                        "MIR variant constructor must initialize its planned case tag",
                    )
                    .into_iter()
                    .map(|error| error.with_module(function.symbol.module)),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;

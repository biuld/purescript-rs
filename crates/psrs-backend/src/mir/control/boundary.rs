//! The MIR boundary checker: the replacement for the retired recognizers.
//!
//! It reconstructs the set of machine dispatch arms an emitted MIR function
//! performs and compares it with the checked construction plan's dispatch arms.
//! Construction owns the machine semantics ([DEC-21]); this re-checks that the
//! lowered function still dispatches exactly the planned cases.
//!
//! Terminal outcomes are deliberately not compared here. Self tail calls are
//! loopified into loop back-edges, so the plan's `Advance`/`Tail` terminals do
//! not survive as MIR `ReturnCall`s; the CC boundary checker owns terminal
//! comparison before loopification. Generic MIR verification still checks
//! types, layout, casts and CFG.

use crate::BackendError;
use crate::cc::{Family, Shape, plan_shape_for};
use crate::mir::value_identity::{definitions, reference_origin};
use crate::mir::{Function, Instruction as I, Module, Terminator as T};
use crate::types::DefinedTypeId;
use psrs_hir::SymbolId;
use std::collections::{BTreeSet, HashMap, HashSet};

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = module.layout.as_ref() else {
        return;
    };
    let table = layout.representation_table();
    let Some(schema) = table.control_schema() else {
        return;
    };
    let mut families = HashMap::new();
    for (repr, family) in [
        (schema.computation, Family::Computation),
        (schema.sequence, Family::Sequence),
        (schema.context, Family::Context),
        (schema.extent, Family::Extent),
        (schema.owner, Family::Owner),
    ] {
        if let Ok(index) = layout.repr_index(repr) {
            families.insert(index, family);
        }
    }
    for function in &module.functions {
        let Some(expected) = plan_shape_for(function.symbol, table) else {
            continue;
        };
        let Some(expected_targets) = crate::cc::plan_external_targets(function.symbol, table)
        else {
            continue;
        };
        let dispatched = arms(function, &families) == flatten(&expected);
        // Non-self tail calls survive loopification as `ReturnCall`s; self tail
        // calls become loop back-edges and are excluded from the plan targets.
        let terminated = external_targets(function) == expected_targets;
        if !dispatched || !terminated {
            errors.push(BackendError::invalid_ir(
                "P9 MIR verification",
                function.span,
                "emitted machine dispatch or outcome disagrees with the checked plan",
            ));
        }
    }
}

fn flatten(shape: &Shape) -> BTreeSet<(Family, u32)> {
    let mut set = BTreeSet::new();
    collect(shape, &mut set);
    set
}

fn collect(shape: &Shape, set: &mut BTreeSet<(Family, u32)>) {
    if let Shape::Switch(family, arms) = shape {
        for (tag, arm) in arms {
            set.insert((*family, *tag));
            collect(arm, set);
        }
    }
}

/// The non-self `ReturnCall` targets in this function.
fn external_targets(function: &Function) -> HashSet<SymbolId> {
    function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            Some(T::ReturnCall {
                function: target, ..
            }) if *target != function.symbol => Some(*target),
            _ => None,
        })
        .collect()
}

/// The machine dispatch arms reachable in this function, across all blocks.
fn arms(function: &Function, families: &HashMap<DefinedTypeId, Family>) -> BTreeSet<(Family, u32)> {
    let definitions = definitions(function);
    let mut set = BTreeSet::new();
    for block in &function.blocks {
        let Some(T::Switch { value, cases, .. }) = &block.terminator else {
            continue;
        };
        let selector = reference_origin(*value, &definitions);
        let Some(I::StructGet {
            type_index,
            field: 0,
            ..
        }) = definitions.get(&selector).copied()
        else {
            continue;
        };
        if let Some(family) = families.get(type_index) {
            for (tag, _) in cases {
                set.insert((*family, *tag as u32));
            }
        }
    }
    set
}

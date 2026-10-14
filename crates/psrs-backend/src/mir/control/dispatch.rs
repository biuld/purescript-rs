//! Recheck dispatch coverage and selected variant projections over real CFG edges.
use crate::BackendError;
use crate::cc::{ControlHelperRole as R, ReprId};
use crate::control::dispatch::Carrier;
use crate::mir::branch_facts::{compute, tag_source};
use crate::mir::value_identity::{definitions, loop_header, loop_parameters, reference_origin};
use crate::mir::{BlockId, Function, Instruction as I, Module, Terminator as T};
use crate::types::DefinedTypeId;
use std::collections::{HashMap, HashSet};

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else {
        return;
    };
    let table = layout.representation_table();
    for helper in &table.control_helpers {
        let role = helper.role();
        if !matches!(role, R::Executor | R::CloneSequence | R::RebuildSequence) {
            continue;
        }
        let Some(function) = module
            .functions
            .iter()
            .find(|f| f.symbol == helper.function())
        else {
            continue;
        };
        let result = (|| {
            let schema = table
                .control_checkpoint_schema()
                .ok_or("control dispatch has no checked schema")?;
            let queue = table
                .control_sequence()
                .ok_or("control dispatch has no checked queue schema")?;
            let carriers = [
                (schema.computation, Carrier::Computation),
                (schema.context, Carrier::Context),
                (
                    queue,
                    if role == R::RebuildSequence {
                        Carrier::Reverse
                    } else {
                        Carrier::Queue
                    },
                ),
            ];
            let mut bases = HashMap::new();
            let mut cases = HashMap::new();
            for (repr, carrier) in carriers {
                let base = layout
                    .repr_index(repr)
                    .map_err(|_| "control dispatch has no physical base")?;
                immutable_tag(module, base)?;
                bases.insert(base, carrier);
                let Some(crate::cc::Representation::Variant { cases: variants }) =
                    table.representation(repr)
                else {
                    return Err("control dispatch has no variant schema");
                };
                for variant in variants {
                    let tag = variant.tag;
                    let case = layout
                        .variant_index(repr, tag)
                        .map_err(|_| "control dispatch has no physical case")?;
                    immutable_tag(module, case)?;
                    cases.insert(case, (base, tag as i32));
                }
            }
            let repr: ReprId = if role == R::Executor {
                schema.computation
            } else {
                queue
            };
            let entry_type = layout
                .repr_index(repr)
                .map_err(|_| "control dispatch has no physical input")?;
            verify_function(function, role, entry_type, &bases, &cases)
        })();
        if let Err(message) = result {
            errors.push(
                BackendError::invalid_ir("P9 MIR control dispatch", function.span, message)
                    .with_module(function.symbol.module),
            );
        }
    }
}

fn verify_function(
    function: &Function,
    role: R,
    entry_type: DefinedTypeId,
    bases: &HashMap<DefinedTypeId, Carrier>,
    cases: &HashMap<DefinedTypeId, (DefinedTypeId, i32)>,
) -> Result<(), &'static str> {
    let definitions = definitions(function);
    let origin = |v| reference_origin(v, &definitions);
    let parameters = loop_parameters(function);
    let index = if role == R::Executor { 2 } else { 0 };
    let input = parameters
        .get(index)
        .copied()
        .ok_or("control dispatch has an invalid parameter contract")?;
    let states = compute(function);
    let loop_header = loop_header(function);
    let header = loop_header.unwrap_or(function.entry);
    // The entry block is the loop preheader only when `loop_header` recognizes
    // its initialization jump. That single edge starts the first iteration and
    // is not a completed iteration, so it does not require the dispatch to
    // dominate it; every other jump to the header is a real backedge.
    let preheader = loop_header.map(|_| function.entry);
    let dominators = crate::mir::cfg::dominators(function);
    let mut dispatches = Vec::new();
    for block in &function.blocks {
        let Some(facts) = states.get(&block.id) else {
            continue;
        };
        for instruction in &block.instructions {
            if let I::StructGet {
                type_index,
                field,
                value,
                ..
            } = instruction
                && *field > 0
                && let Some((base, tag)) = cases.get(type_index)
            {
                let owner = origin(*value);
                let known = match definitions.get(&owner).copied() {
                    Some(I::StructNew {
                        type_index: actual,
                        arguments,
                        ..
                    }) if actual == type_index => arguments.first().and_then(|v| match definitions
                        .get(&origin(*v))
                        .copied()
                    {
                        Some(I::Constant { value, .. }) => Some(*value),
                        _ => None,
                    }),
                    _ => facts.get(&(*base, owner)).copied(),
                };
                if known.is_some_and(|known| known != *tag) {
                    return Err(
                        "control projection must match the selected case of its actual input",
                    );
                }
            }
        }
        if let Some(T::Switch {
            value,
            cases,
            default,
            ..
        }) = &block.terminator
            && let Some((base, owner)) = tag_source(*value, &definitions)
            && let Some(carrier) = bases.get(&base)
        {
            carrier.verify(cases.iter().map(|(tag, _)| *tag))?;
            if !must_trap(function, *default, &mut HashSet::new()) {
                return Err("control dispatch must trap on unsupported alternatives");
            }
            if base == entry_type {
                if role == R::Executor && owner != origin(input) {
                    return Err("control dispatch must select the current helper input");
                }
                // The selected input dispatch is reached on every iteration,
                // before any branch's executor work can become observable.
                if owner == origin(input)
                    && dominators
                        .get(&block.id)
                        .is_some_and(|d| d.contains(&header))
                {
                    dispatches.push(block.id);
                }
            }
        }
    }
    if !dispatches.into_iter().any(|dispatch| {
        function
            .blocks
            .iter()
            .filter(|b| states.contains_key(&b.id))
            .all(|block| {
                let invokes = block
                    .instructions
                    .iter()
                    .any(crate::mir::effects::may_invoke);
                let finishes = matches!(
                    &block.terminator,
                    Some(T::Return { .. } | T::ReturnCall { .. } | T::ReturnCallRef { .. })
                ) || matches!(&block.terminator, Some(T::Jump { target, .. })
                    if *target == header && Some(block.id) != preheader);
                if !invokes && !finishes {
                    return true;
                }
                // A terminator dispatch follows this block's instructions, so it
                // cannot justify a call earlier in the same block.
                (!invokes || block.id != dispatch)
                    && dominators
                        .get(&block.id)
                        .is_some_and(|d| d.contains(&dispatch))
            })
    }) {
        return Err("control input dispatch must dominate execution and normal transitions");
    }
    Ok(())
}

fn immutable_tag(module: &Module, id: DefinedTypeId) -> Result<(), &'static str> {
    let definition = module
        .types
        .iter()
        .flat_map(|group| &group.0)
        .nth(id.0 as usize);
    if !matches!(definition.map(|d| &d.composite),
    Some(crate::types::CompositeType::Struct(fields)) if fields.first() == Some(&crate::types::FieldType {
        storage: crate::types::StorageType::I32, mutable: false,
    })) {
        return Err("control dispatch requires an immutable i32 tag field");
    }
    Ok(())
}

fn must_trap(function: &Function, id: BlockId, active: &mut HashSet<BlockId>) -> bool {
    if !active.insert(id) {
        return false;
    }
    let result = function
        .blocks
        .iter()
        .find(|b| b.id == id)
        .is_some_and(|block| {
            if block
                .instructions
                .iter()
                .any(|i| matches!(i, I::Unreachable { .. }))
            {
                return true;
            }
            match &block.terminator {
                Some(T::Trap { .. }) => true,
                Some(t @ (T::Jump { .. } | T::Branch { .. } | T::Switch { .. })) => {
                    crate::mir::cfg::successors(t)
                        .iter()
                        .all(|id| must_trap(function, *id, active))
                }
                _ => false,
            }
        });
    active.remove(&id);
    result
}

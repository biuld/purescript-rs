//! Root initialization and normal completion after physical lowering.
use crate::BackendError;
use crate::cc::{ControlHelperRole as R, RepresentationTable};
use crate::control_provenance::root_extent_relation;
use crate::mir::value_identity::{definitions, fresh_iteration_allocation, reference_origin};
use crate::mir::{Function, Instruction as I, Module, Terminator as T};
use crate::types::DefinedTypeId;
use psrs_hir::Intrinsic;
use std::collections::HashMap;

struct Schema {
    context: DefinedTypeId,
    boundary: DefinedTypeId,
    empty: DefinedTypeId,
    status: DefinedTypeId,
}

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else { return };
    let table = layout.representation_table();
    if table.control_helpers.is_empty() {
        return;
    }
    let schema = table
        .control_checkpoint_schema()
        .zip(table.control_replay_schema())
        .and_then(|(checkpoint, replay)| {
            Some(Schema {
                context: layout.variant_index(checkpoint.context, 0).ok()?,
                boundary: layout.variant_index(replay.boundary, 0).ok()?,
                empty: layout.variant_index(replay.sequence, 0).ok()?,
                status: layout.repr_index(replay.status).ok()?,
            })
        });
    let Some(schema) = schema else {
        errors.push(BackendError::invalid_ir(
            "P9 MIR control root",
            module.span,
            "control run has no physical root schema",
        ));
        return;
    };
    for function in &module.functions {
        if let Err(message) = verify_function(function, table, &schema) {
            errors.push(
                BackendError::invalid_ir("P9 MIR control root", function.span, message)
                    .with_module(function.symbol.module),
            );
        }
    }
}

fn verify_function(
    function: &Function,
    table: &RepresentationTable,
    schema: &Schema,
) -> Result<(), &'static str> {
    let definitions = definitions(function);
    let origin = |value| reference_origin(value, &definitions);
    let role = |id: crate::cc::ControlHelperId| {
        table
            .control_helpers
            .get(id.0 as usize)
            .map(|helper| helper.role())
    };
    let constant = |value, expected| matches!(definitions.get(&origin(value)).copied(), Some(I::Constant { value, .. }) if *value == expected);
    let physical_root = |value| {
        let Some(I::StructNew {
            type_index,
            arguments,
            ..
        }) = definitions.get(&origin(value)).copied()
        else {
            return Err("control Start must return a freshly allocated root extent");
        };
        if *type_index != schema.boundary || arguments.len() != 2 || !constant(arguments[0], 0) {
            return Err("control Start must return a freshly allocated root extent");
        }
        let status = origin(arguments[1]);
        let live = match definitions.get(&status).copied() {
            Some(I::ArrayNew {
                type_index,
                elements,
                ..
            }) => *type_index == schema.status && elements.len() == 1 && constant(elements[0], 0),
            Some(I::ArrayNewFilled {
                type_index,
                length,
                value,
                ..
            }) => *type_index == schema.status && constant(*length, 1) && constant(*value, 0),
            _ => false,
        };
        if !live {
            return Err("control Start needs a fresh one-element live status cell");
        }
        Ok(status)
    };
    if table
        .control_helpers
        .iter()
        .any(|helper| helper.function() == function.symbol && helper.role() == R::Start)
    {
        if !function.parameters.is_empty() {
            return Err("control Start has an invalid parameter contract");
        }
        let mut returns = 0;
        for block in &function.blocks {
            match &block.terminator {
                Some(T::Return { value, .. }) => {
                    returns += 1;
                    physical_root(*value)?;
                }
                Some(T::ReturnCall { .. } | T::ReturnCallRef { .. }) => {
                    return Err("control Start must return a freshly allocated root extent");
                }
                _ => {}
            }
        }
        if returns == 0 {
            return Err("control Start must return a freshly allocated root extent");
        }
    }
    if !definitions.values().any(|instruction| matches!(instruction, I::ControlEvidence { contract, .. }
        if table.control_contracts.get(contract.0 as usize).is_some_and(|proof| proof.operation() == Intrinsic::ControlRun))) {
        return Ok(());
    }
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
    let mut owners = HashMap::<_, Vec<_>>::new();
    for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
        let I::ControlEvidence {
            contract,
            implementation,
            ..
        } = instruction
        else {
            continue;
        };
        if !table
            .control_contracts
            .get(contract.0 as usize)
            .is_some_and(|proof| proof.operation() == Intrinsic::ControlRun)
        {
            continue;
        }
        let implementation = origin(*implementation);
        let Some(I::Call {
            contract: Some(id),
            arguments,
            ..
        }) = definitions.get(&implementation).copied()
        else {
            return Err("control run has no local executor invocation");
        };
        if role(*id) != Some(R::Executor) || arguments.len() != 6 {
            return Err("control run has no local executor invocation");
        }
        if !matches!(definitions.get(&origin(arguments[3])).copied(), Some(I::StructNew { type_index, arguments, .. })
            if *type_index == schema.empty && arguments.len() == 1 && constant(arguments[0], 0))
        {
            return Err("control run must start with an empty root queue");
        }
        let extent = origin(arguments[4]);
        let retained_start = matches!(definitions.get(&extent).copied(), Some(I::Call { contract: Some(id), arguments, .. }) if role(*id) == Some(R::Start) && arguments.is_empty());
        if !retained_start
            && !matches!(definitions.get(&extent).copied(), Some(I::StructNew { type_index, .. }) if *type_index == schema.boundary)
        {
            return Err("control run must start with a fresh Start extent");
        }
        let point = *points
            .get(&implementation)
            .ok_or("control run has no local executor invocation")?;
        let status = if retained_start {
            None
        } else {
            Some(physical_root(extent)?)
        };
        if !fresh_iteration_allocation(function, extent, point, &points, &dominators)
            || status.is_some_and(|status| {
                !fresh_iteration_allocation(function, status, point, &points, &dominators)
            })
        {
            return Err(
                "control run root must be allocated in its current invocation or iteration",
            );
        }
        for owned in std::iter::once(extent).chain(status) {
            let producer = *points
                .get(&owned)
                .ok_or("control run has no root producer")?;
            let previous = owners.entry(owned).or_default();
            for previous_point in previous
                .iter()
                .copied()
                .filter(|previous| *previous != point)
            {
                let forward =
                    crate::mir::cfg::reachable_without(function, previous_point, point, producer);
                let backward =
                    crate::mir::cfg::reachable_without(function, point, previous_point, producer);
                if forward != Some(false) || backward != Some(false) {
                    return Err(
                        "control run factories must not reuse another invocation's root extent or status cell",
                    );
                }
            }
            previous.push(point);
        }
        let Some(I::StructNew {
            type_index,
            arguments: context,
            ..
        }) = definitions.get(&origin(arguments[5])).copied()
        else {
            return Err("control run must install a root dispatch context");
        };
        if *type_index != schema.context || context.len() != 2 || !constant(context[0], 0) {
            return Err("control run must install a root dispatch context");
        }
        root_extent_relation(extent, origin(context[1]))?;
        let closed = crate::mir::cfg::normal_paths_cross(
            function,
            point,
            |instruction| {
                matches!(instruction,
                I::Call { contract: Some(id), arguments, .. }
                    if role(*id) == Some(R::Close) && arguments.len() == 1 && origin(arguments[0]) == extent)
            },
            |instruction| matches!(instruction, I::ControlEvidence { implementation: actual, .. } if origin(*actual) == implementation),
        );
        if !closed {
            return Err(
                "control run must close its root after execution and before publishing its result",
            );
        }
    }
    Ok(())
}

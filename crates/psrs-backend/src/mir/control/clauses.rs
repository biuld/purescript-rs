//! A source return or operation applies its own scoped clause in MIR.
//!
//! The executor's input dispatch selects a source step; within that step's arm,
//! every call of the current scoped context's clause must be the clause that step
//! owns. The scoped exit must consume the complete application result and its
//! calls must dominate that exit. Checks use real CFG identities and edges.
use crate::BackendError;
use crate::cc::ControlHelperRole as R;
use crate::control::provenance::{
    ClauseKind, FinishProjection, clause_operand_relation, clause_selection_relation,
};
use crate::mir::value_identity::{
    definitions, loop_header, loop_parameters, precedes, reference_origin, step_definitions,
};
use crate::mir::{Function, Instruction as I, Module, Terminator as T};
use crate::types::DefinedTypeId;
use std::collections::{HashSet, VecDeque};

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else {
        return;
    };
    let table = layout.representation_table();
    let Some(helper) = table
        .control_helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
    else {
        return;
    };
    let Some(function) = module
        .functions
        .iter()
        .find(|function| function.symbol == helper.function())
    else {
        return;
    };
    let result = (|| {
        let schema = table
            .control_checkpoint_schema()
            .ok_or("clause selection has no checked schema")?;
        let scoped = layout
            .variant_index(schema.context, 1)
            .map_err(|_| "clause selection has no scoped case")?;
        let computation = layout
            .repr_index(schema.computation)
            .map_err(|_| "clause selection has no computation base")?;
        let context_base = layout
            .repr_index(schema.context)
            .map_err(|_| "clause selection has no context base")?;
        let payloads = [0, 1].map(|tag| layout.variant_index(schema.computation, tag).ok());
        let payloads = payloads[0]
            .zip(payloads[1])
            .ok_or("clause selection has no source payload cases")?;
        let context = loop_parameters(function)
            .get(5)
            .copied()
            .ok_or("clause selection has an invalid executor contract")?;
        verify_function(
            function,
            scoped,
            computation,
            context,
            context_base,
            payloads,
            table
                .control_resumption
                .as_ref()
                .ok_or("clause selection has no raw resumption contract")?
                .function(),
        )
    })();
    if let Err(message) = result {
        errors.push(
            BackendError::invalid_ir("P9 MIR clause selection", function.span, message)
                .with_module(function.symbol.module),
        );
    }
}

fn verify_function(
    function: &Function,
    scoped: DefinedTypeId,
    computation: DefinedTypeId,
    context: crate::types::ValueId,
    context_base: DefinedTypeId,
    payloads: (DefinedTypeId, DefinedTypeId),
    raw: psrs_hir::SymbolId,
) -> Result<(), &'static str> {
    let definitions = definitions(function);
    let origin = |value| reference_origin(value, &definitions);
    let Some((dispatch, cases)) =
        function
            .blocks
            .iter()
            .find_map(|block| match &block.terminator {
                Some(T::Switch { value, cases, .. })
                    if crate::mir::branch_facts::tag_source(*value, &definitions)
                        .is_some_and(|(base, _)| base == computation) =>
                {
                    Some((block.id, cases))
                }
                _ => None,
            })
    else {
        return Ok(());
    };
    for kind in [ClauseKind::Return, ClauseKind::Operation] {
        let Some(target) = cases
            .iter()
            .find(|(tag, _)| *tag == kind.source_tag())
            .map(|(_, block)| *block)
        else {
            continue;
        };
        let mut seen = HashSet::new();
        let mut pending = VecDeque::from([target]);
        let mut steps = 0;
        while let Some(id) = pending.pop_front() {
            if id == dispatch || !seen.insert(id) {
                continue;
            }
            let Some(block) = function.blocks.iter().find(|block| block.id == id) else {
                continue;
            };
            for instruction in &block.instructions {
                let I::ClosureCall {
                    function: callee, ..
                } = instruction
                else {
                    continue;
                };
                let Some(I::StructGet {
                    type_index,
                    field,
                    value,
                    ..
                }) = definitions.get(&origin(*callee)).copied()
                else {
                    continue;
                };
                if *type_index != scoped {
                    continue;
                }
                let Some(field) = field.checked_sub(1) else {
                    continue;
                };
                clause_selection_relation(
                    kind,
                    origin(context),
                    Some(FinishProjection {
                        owner: origin(*value),
                        field,
                    }),
                )?;
            }
            if let Some(terminator) = &block.terminator {
                if let T::Switch { value, cases, .. } = terminator
                    && crate::mir::branch_facts::tag_source(*value, &definitions)
                        == Some((context_base, origin(context)))
                {
                    let scoped_target = cases
                        .iter()
                        .find(|(tag, _)| *tag == 1)
                        .map(|(_, target)| *target)
                        .ok_or("source step has no scoped clause path")?;
                    verify_step(
                        function,
                        scoped_target,
                        kind,
                        scoped,
                        if kind == ClauseKind::Return {
                            payloads.0
                        } else {
                            payloads.1
                        },
                        raw,
                    )?;
                    steps += 1;
                }
                for successor in crate::mir::cfg::successors(terminator) {
                    if successor != dispatch {
                        pending.push_back(successor);
                    }
                }
            }
        }
        if steps == 0 {
            return Err("source step has no scoped clause path");
        }
    }
    Ok(())
}

fn verify_step(
    function: &Function,
    start: crate::mir::BlockId,
    kind: ClauseKind,
    scoped: DefinedTypeId,
    payload_type: DefinedTypeId,
    raw: psrs_hir::SymbolId,
) -> Result<(), &'static str> {
    let definitions = step_definitions(function, start);
    let origin = |value| reference_origin(value, &definitions);
    let parameters = loop_parameters(function);
    let dominators = crate::mir::cfg::dominators_from(function, start, loop_header(function));
    let points = function
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .instructions
                .iter()
                .enumerate()
                .filter_map(move |(index, i)| {
                    i.destination().map(|value| (value, (block.id, index)))
                })
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut pending = VecDeque::from([start]);
    let mut seen = HashSet::new();
    let mut transitions = 0;
    while let Some(id) = pending.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        let block = function
            .blocks
            .iter()
            .find(|block| block.id == id)
            .ok_or("source clause path has no block")?;
        // Only an actual exit establishes a transition. An earlier call whose
        // result is discarded cannot discharge the scoped step obligation.
        let transition = match &block.terminator {
            Some(T::Jump {
                target, arguments, ..
            }) if Some(*target) == loop_header(function) => {
                Some((block.instructions.len(), arguments.as_slice()))
            }
            Some(T::ReturnCall {
                function: callee,
                arguments,
                ..
            }) if *callee == function.symbol => {
                Some((block.instructions.len(), arguments.as_slice()))
            }
            Some(T::Return { value, .. }) => {
                match definitions.get(&origin(*value)).copied() {
                    Some(I::Call {
                        function: callee,
                        arguments,
                        ..
                    }) if *callee == function.symbol => {
                        let point = points
                            .get(&origin(*value))
                            .copied()
                            .ok_or("source clause transition has no execution point")?;
                        if !precedes(point, (id, block.instructions.len()), &dominators) {
                            return Err("source clause transition must dominate its actual exit");
                        }
                        // Use the call's point for application dominance below.
                        Some((point.1, arguments.as_slice()))
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some((index, arguments)) = transition {
            if arguments.len() != 6 {
                return Err("source clause result must enter the next executor step");
            }
            if origin(arguments[0]) != origin(parameters[0])
                || origin(arguments[1]) != origin(parameters[1])
            {
                return Err("source clause transition must preserve its state and root handler");
            }
            let mut result = origin(arguments[2]);
            let mut consumer = match &block.terminator {
                Some(T::Return { value, .. }) => points[&origin(*value)],
                _ => (id, index),
            };
            let mut payload = None;
            let mut resumption = None;
            for _ in 0..(if kind == ClauseKind::Return { 1 } else { 2 }) {
                let Some(I::ClosureCall {
                    function: callee,
                    arguments,
                    ..
                }) = definitions.get(&result).copied()
                else {
                    return Err("source clause result must enter the next executor step");
                };
                let point = points
                    .get(&result)
                    .copied()
                    .ok_or("source clause application has no execution point")?;
                if arguments.len() != 1
                    || !precedes(point, consumer, &dominators)
                    || !dominators
                        .get(&point.0)
                        .is_some_and(|set| set.contains(&start))
                {
                    return Err("source clause application must execute in its scoped step");
                }
                payload = Some(origin(arguments[0]));
                resumption.get_or_insert(origin(arguments[0]));
                result = origin(*callee);
                consumer = point;
            }
            let projection = |value, ty| match definitions.get(&value).copied() {
                Some(I::StructGet {
                    type_index,
                    field,
                    value,
                    ..
                }) if *type_index == ty && *field > 0 => Some(FinishProjection {
                    owner: origin(*value),
                    field: field - 1,
                }),
                _ => None,
            };
            clause_selection_relation(kind, origin(parameters[5]), projection(result, scoped))?;
            clause_operand_relation(
                origin(parameters[2]),
                projection(payload.unwrap(), payload_type),
            )?;
            if kind == ClauseKind::Operation
                && !matches!(definitions.get(&resumption.unwrap()).copied(), Some(I::ClosureNew { function: target, .. }) if *target == raw)
            {
                return Err("source operation clause must receive its raw resumption");
            }
            transitions += 1;
            continue;
        }
        match &block.terminator {
            Some(T::Trap { .. }) => {}
            Some(T::Return { .. } | T::ReturnCall { .. } | T::ReturnCallRef { .. }) => {
                return Err("source clause result must enter the next executor step");
            }
            Some(terminator) => pending.extend(crate::mir::cfg::successors(terminator)),
            None => return Err("source clause path has no exit"),
        }
    }
    if transitions == 0 {
        return Err("source clause result must enter the next executor step");
    }
    Ok(())
}

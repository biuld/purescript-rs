//! Publication witnesses survive ordinary inlining as physical frame origins.
use super::execution::View;
use crate::control::execution::Operands;
use crate::control::owned::{self, Expected, OwnedSchema};
use crate::mir::value_identity::{definitions, reference_origin};
use crate::mir::{BlockId, Function, Instruction as I, Module, Terminator as T};
use psrs_hir::Intrinsic;
use std::collections::{HashMap, HashSet};

type Point = (BlockId, usize);

pub(super) fn verify(module: &Module) -> Result<(), Vec<crate::BackendError>> {
    let Some(layout) = &module.layout else {
        return Ok(());
    };
    let table = layout.representation_table();
    if table.control_helpers.is_empty() {
        return Ok(());
    }
    let total = crate::mir::effects::total_functions(module);
    let mut errors = Vec::new();
    for function in &module.functions {
        let checked: Result<(), &'static str> = (|| {
            let definitions = definitions(function);
            let points =
                function
                    .blocks
                    .iter()
                    .flat_map(|block| {
                        block.instructions.iter().enumerate().filter_map(
                            move |(index, instruction)| {
                                instruction
                                    .destination()
                                    .map(|value| (value, (block.id, index)))
                            },
                        )
                    })
                    .collect::<HashMap<_, _>>();
            let origin = |value| reference_origin(value, &definitions);
            let raw = table
                .control_resumption
                .as_ref()
                .ok_or("publication has no raw contract")?
                .function();
            let empty = HashMap::new();
            let view = View {
                definitions: &definitions,
                aliases: &empty,
                layout,
                raw,
            };
            for block in &function.blocks {
                for (index, instruction) in block.instructions.iter().enumerate() {
                    let I::ControlEvidence {
                        contract,
                        implementation,
                        arguments,
                        ..
                    } = instruction
                    else {
                        continue;
                    };
                    let Some(proof) = table.control_contracts.get(contract.0 as usize) else {
                        continue;
                    };
                    let mut end = (block.id, index);
                    let implementation = origin(*implementation);
                    let (roots, cells) = match proof.operation() {
                        Intrinsic::ControlOnce => {
                            if matches!(definitions.get(&implementation), Some(I::Call { .. })) {
                                continue;
                            }
                            let owned = OwnedSchema::checked(table)?;
                            let fields = view
                                .node(implementation, owned.owner, Some(0))
                                .filter(|fields| fields.len() == 2)
                                .ok_or("inlined Once must publish its owner frame")?;
                            if arguments.len() != 1 {
                                return Err("Once has invalid callback evidence");
                            }
                            (
                                vec![implementation],
                                vec![
                                    (fields[0], owned.callback, Expected::Value(arguments[0])),
                                    (fields[1], owned.status, Expected::Integer(0)),
                                ],
                            )
                        }
                        Intrinsic::ControlRun => {
                            let Some(I::Call { arguments, .. }) =
                                definitions.get(&implementation).copied()
                            else {
                                continue;
                            };
                            if arguments.len() != 6 {
                                continue;
                            }
                            let extent = origin(arguments[4]);
                            if matches!(definitions.get(&extent), Some(I::Call { .. })) {
                                continue;
                            }
                            let replay = table
                                .control_replay_schema()
                                .ok_or("publication has no extent schema")?;
                            let fields = view
                                .node(extent, replay.boundary, Some(0))
                                .filter(|fields| fields.len() == 1)
                                .ok_or("inlined Start must publish its root frame")?;
                            end = *points
                                .get(&implementation)
                                .ok_or("publication has no executor point")?;
                            (
                                arguments.clone(),
                                vec![(fields[0], replay.status, Expected::Integer(0))],
                            )
                        }
                        _ => continue,
                    };
                    let producers = cells
                        .iter()
                        .map(|(cell, _, _)| {
                            points
                                .get(&origin(*cell))
                                .copied()
                                .ok_or("publication has no private cell producer")
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    // Inlining inserts the whole factory at one call site. Its
                    // fresh private cells must share that dominating block.
                    let start_block = producers[0].0;
                    if producers.iter().any(|point| point.0 != start_block) {
                        return Err("inlined factory cells must share their allocation site");
                    }
                    let start = (
                        start_block,
                        producers.iter().map(|point| point.1).min().unwrap(),
                    );
                    let window = window(function, start, end)?;
                    let paths = crate::mir::branch_facts::paths(&window, start_block, None)?;
                    if paths.is_empty() {
                        return Err("inlined factory has no publication path");
                    }
                    for path in paths.into_iter().filter(|path| path.block == end.0) {
                        if path.trapped {
                            return Err("inlined factory traps before publication");
                        }
                        let view = View {
                            definitions: &definitions,
                            aliases: &path.aliases,
                            layout,
                            raw,
                        };
                        let mut observed = super::traces::events(&window, layout, &path)?;
                        if proof.operation() == Intrinsic::ControlRun {
                            // An exclusive caller can build both candidate Run
                            // contexts before choosing an executor. These ordinary
                            // caller allocations do not alter the private status.
                            observed.retain(|event| {
                                !matches!(event, crate::control::events::Event::Allocate { .. })
                            });
                        }
                        owned::publication(&view, &roots, &cells, &observed, &total)?;
                    }
                }
            }
            Ok(())
        })();
        if let Err(message) = checked {
            errors.push(
                crate::BackendError::invalid_ir(
                    "P9 MIR control publication",
                    function.span,
                    message,
                )
                .with_module(function.symbol.module),
            );
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Restrict to paths between the allocation and its publication. Other branches
/// can have their own Run witnesses; they do not belong to this invocation.
fn window(function: &Function, start: Point, end: Point) -> Result<Function, &'static str> {
    let mut reaches = HashSet::from([end.0]);
    loop {
        let before = reaches.len();
        for block in &function.blocks {
            if block.terminator.as_ref().is_some_and(|t| {
                crate::mir::cfg::successors(t)
                    .iter()
                    .any(|id| reaches.contains(id))
            }) {
                reaches.insert(block.id);
            }
        }
        if before == reaches.len() {
            break;
        }
    }
    if !reaches.contains(&start.0) {
        return Err("private allocation cannot reach publication");
    }
    let mut window = function.clone();
    window.entry = start.0;
    window.blocks.retain(|block| reaches.contains(&block.id));
    for block in &mut window.blocks {
        if block.id == end.0 {
            block.instructions.truncate(end.1);
            block.terminator = Some(T::Return {
                value: function.result,
                span: function.span,
            });
        }
        if block.id == start.0 {
            block.instructions.drain(..start.1);
        }
    }
    // Off-path successors cannot publish this witness. Preserve their edge IDs
    // as empty exceptional exits without traversing another invocation.
    for block in &function.blocks {
        if !reaches.contains(&block.id) {
            window.blocks.push(crate::mir::BasicBlock {
                id: block.id,
                parameters: block.parameters.clone(),
                instructions: vec![],
                terminator: Some(T::Trap {
                    span: function.span,
                }),
            });
        }
    }
    Ok(window)
}

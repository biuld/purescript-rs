//! Freshness and published contents are checked independently after physical layout.
use super::execution::View;
use crate::BackendError;
use crate::cc::ControlHelperRole as R;
use crate::control::execution::Operands;
use crate::control::owned::{self, Expected, OwnedSchema};
use crate::mir::value_identity::definitions;
use crate::mir::{Module, Terminator as T};

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else { return };
    let table = layout.representation_table();
    let total = crate::mir::effects::total_functions(module);
    for helper in &table.control_helpers {
        let role = helper.role();
        if !matches!(role, R::Start | R::Once | R::Cancel) {
            continue;
        }
        let Some(function) = module
            .functions
            .iter()
            .find(|function| function.symbol == helper.function())
        else {
            continue;
        };
        let result = (|| {
            let replay = table
                .control_replay_schema()
                .ok_or("control factory has no private cell schema")?;
            let owned = OwnedSchema::checked(table)?;
            let definitions = definitions(function);
            let exits = crate::mir::branch_facts::paths(function, function.entry, None)?;
            let normal = exits
                .iter()
                .filter(|path| !path.trapped)
                .map(|path| super::traces::events(function, layout, path))
                .collect::<Result<Vec<_>, _>>()?;
            if normal.is_empty() {
                return Err("control factory has no normal publication path");
            }
            for exit in exits {
                let view = View {
                    definitions: &definitions,
                    layout,
                    aliases: &exit.aliases,
                    raw: table
                        .control_resumption
                        .as_ref()
                        .ok_or("control factory has no raw contract")?
                        .function(),
                };
                if exit.trapped {
                    crate::control::events::exceptional(
                        &view,
                        &super::traces::events(function, layout, &exit)?,
                        &normal,
                        &crate::control::events::carriers(table)?,
                        replay.status,
                    )?;
                    continue;
                }
                let block = function
                    .blocks
                    .iter()
                    .find(|block| block.id == exit.block)
                    .ok_or("control factory has no exit block")?;
                let T::Return { value, .. } = block
                    .terminator
                    .as_ref()
                    .ok_or("control factory has no exit")?
                else {
                    return Err("control factory must publish its planned fresh frame");
                };
                if role == R::Cancel {
                    if function.parameters.len() != 1 {
                        return Err("cancellation has invalid parameters");
                    }
                    owned::cancel(
                        &owned,
                        &view,
                        function.parameters[0],
                        *value,
                        &super::traces::events(function, layout, &exit)?,
                    )?;
                    continue;
                }
                let repr = if role == R::Start {
                    replay.boundary
                } else {
                    owned.owner
                };
                let fields = view
                    .node(*value, repr, Some(0))
                    .ok_or("control factory must publish its planned fresh frame")?;
                let cells = if role == R::Start && fields.len() == 1 {
                    vec![(fields[0], replay.status, Expected::Integer(0))]
                } else if role == R::Once && fields.len() == 2 && function.parameters.len() == 1 {
                    vec![
                        (
                            fields[0],
                            owned.callback,
                            Expected::Value(function.parameters[0]),
                        ),
                        (fields[1], owned.status, Expected::Integer(0)),
                    ]
                } else {
                    return Err("control factory has invalid frame fields");
                };
                owned::publication(
                    &view,
                    &[*value],
                    &cells,
                    &super::traces::events(function, layout, &exit)?,
                    &total,
                )?;
            }
            Ok(())
        })();
        if let Err(message) = result {
            errors.push(
                BackendError::invalid_ir("P9 MIR control initialization", function.span, message)
                    .with_module(function.symbol.module),
            );
        }
    }
}

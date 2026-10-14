//! Factory freshness and current cell contents are distinct publication facts.
use super::execution::View;
use super::traces::{Trace, paths};
use crate::BackendError;
use crate::cc::value_identity::definitions;
use crate::cc::{ControlHelperRole as R, Function, RepresentationTable};
use crate::control::execution::Operands;
use crate::control::owned::{self, Expected, OwnedSchema};

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
    functions: Option<&std::collections::HashMap<psrs_hir::SymbolId, &Function>>,
) -> Result<(), Vec<BackendError>> {
    let Some(role @ (R::Start | R::Once | R::Cancel)) = table
        .control_helpers
        .iter()
        .find(|helper| helper.function() == function.symbol)
        .map(|helper| helper.role())
    else {
        return Ok(());
    };
    let total = functions
        .map(super::super::total::functions)
        .unwrap_or_default();
    let result = (|| {
        let replay = table
            .control_replay_schema()
            .ok_or("control factory has no private cell schema")?;
        let owned = OwnedSchema::checked(table)?;
        let definitions = definitions(&function.assignments);
        let paths = paths(
            &function.assignments,
            Trace::default(),
            function.symbol,
            &definitions,
        );
        let normal = paths
            .iter()
            .filter(|path| !path.trapped)
            .map(|path| path.events.clone())
            .collect::<Vec<_>>();
        if normal.is_empty() {
            return Err("control factory has no normal publication path");
        }
        for path in paths {
            if !path.trapped {
                super::refinements::verify(function, table, &path)?;
            }
            let view = View {
                definitions: definitions.clone(),
                aliases: Some(&path.aliases),
                raw: table
                    .control_resumption
                    .as_ref()
                    .ok_or("control factory has no raw contract")?
                    .function(),
            };
            if path.trapped {
                crate::control::events::exceptional(
                    &view,
                    &path.events,
                    &normal,
                    &crate::control::events::carriers(table)?,
                    replay.status,
                )?;
                continue;
            }
            if role == R::Cancel {
                if function.parameters.len() != 1 {
                    return Err("cancellation has invalid parameters");
                }
                owned::cancel(
                    &owned,
                    &view,
                    function.parameters[0],
                    function.result,
                    &path.events,
                )?;
                continue;
            }
            let repr = if role == R::Start {
                replay.boundary
            } else {
                owned.owner
            };
            let fields = view
                .node(function.result, repr, Some(0))
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
            owned::publication(&view, &[function.result], &cells, &path.events, &total)?;
        }
        Ok(())
    })();
    result.map_err(|message| {
        vec![
            BackendError::invalid_ir("P8 CC control initialization", function.span, message)
                .with_module(function.symbol.module),
        ]
    })
}

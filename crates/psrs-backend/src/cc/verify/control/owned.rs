//! Execute the structured body symbolically to retain ordered owner effects.
use super::execution::View;
use crate::BackendError;
use crate::cc::value_identity::definitions;
use crate::cc::{AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable};
use crate::control::execution::{Operands, Schema};
use crate::control::owned::{self, OwnedSchema};

use super::traces::Trace;
use super::traces::paths;
use crate::control::events::Event;

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    if !table
        .control_helpers
        .iter()
        .any(|helper| helper.role() == R::Executor && helper.function() == function.symbol)
    {
        return Ok(());
    }
    let result = (|| {
        let schema = Schema::checked(table)?;
        let owned = OwnedSchema::checked(table)?;
        let definitions = definitions(&function.assignments);
        let paths = paths(
            &function.assignments,
            Trace::default(),
            function.symbol,
            &definitions,
        )
        .into_iter()
        .filter(|path| !path.trapped)
        .collect::<Vec<_>>();
        if paths.is_empty() {
            return Err("owned execution has no normal transfer path");
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
                    .ok_or("owned execution has no raw resumption contract")?
                    .function(),
            };
            if !path.events.iter().any(|event| matches!(event, Event::Select { value, tag: 5 } if view.tag(*value) == Some((schema.computation, view.origin(function.parameters[2]))))) { continue; }
            let Some(A::DirectCall {
                function: target,
                arguments,
                ..
            }) = definitions.get(&view.origin(function.result)).copied()
            else {
                return Err("owned execution must return its executor transition");
            };
            if *target != function.symbol {
                return Err("owned execution must return its executor transition");
            }
            owned::verify(
                &schema,
                &owned,
                &view,
                &function.parameters,
                arguments,
                &path.events,
            )?;
        }
        Ok(())
    })();
    result.map_err(|message| {
        vec![
            BackendError::invalid_ir("P8 CC owned execution", function.span, message)
                .with_module(function.symbol.module),
        ]
    })
}

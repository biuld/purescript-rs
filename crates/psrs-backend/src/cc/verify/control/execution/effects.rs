//! Account for the complete executed path, including prefixes and exceptional exits.
use super::super::traces::{Trace, paths};
use super::View;
use crate::BackendError;
use crate::cc::value_identity::definitions;
use crate::cc::{AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable};
use crate::control::events::{self, Event};
use crate::control::execution::{self, Facts, Operands, Schema};
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
    functions: Option<&HashMap<SymbolId, &Function>>,
) -> Result<(), Vec<BackendError>> {
    if !table
        .control_helpers
        .iter()
        .any(|helper| helper.role() == R::Executor && helper.function() == function.symbol)
    {
        return Ok(());
    }
    let total = functions
        .map(super::super::super::total::functions)
        .unwrap_or_default();
    let checked: Result<(), &'static str> = (|| {
        let schema = Schema::checked(table)?;
        let status = table
            .control_replay_schema()
            .ok_or("execution has no status schema")?
            .status;
        let carriers = events::carriers(table)?;
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
        for path in paths {
            if !path.trapped {
                super::super::refinements::verify(function, table, &path)?;
            }
            let view = View {
                definitions: definitions.clone(),
                aliases: Some(&path.aliases),
                raw: table
                    .control_resumption
                    .as_ref()
                    .ok_or("execution has no raw contract")?
                    .function(),
            };
            if path.trapped {
                events::exceptional(&view, &path.events, &normal, &carriers, status)?;
                continue;
            }
            let mut facts = Facts::new();
            for event in &path.events {
                if let Event::Select { value, tag } = event
                    && let Some(key) = view.tag(*value)
                {
                    facts.insert(key, *tag);
                }
            }
            if facts.get(&(schema.computation, view.origin(function.parameters[2]))) == Some(&5) {
                continue;
            }
            let result = view.origin(function.result);
            let next = match definitions.get(&result).copied() {
                Some(A::DirectCall {
                    function: target,
                    arguments,
                    ..
                }) if *target == function.symbol => Some(arguments.as_slice()),
                _ => None,
            };
            execution::effects(
                &schema,
                &view,
                &facts,
                &function.parameters,
                next,
                &path.events,
                &total,
            )?;
            events::allocations(&view, next.unwrap_or(&[result]), &path.events)?;
        }
        Ok(())
    })();
    checked.map_err(|message| {
        vec![
            BackendError::invalid_ir("P8 CC root and queue effects", function.span, message)
                .with_module(function.symbol.module),
        ]
    })
}

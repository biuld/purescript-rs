//! Verify extent helper bodies rather than trusting their semantic callee IDs.
use super::execution::View;
use super::traces::{Trace, paths};
use crate::BackendError;
use crate::cc::{AssignmentKind as A, ControlHelperRole as R, Function, RepresentationTable};
use crate::control::execution::Operands;
use crate::control::{events, lifecycle};

pub(super) fn verify(
    function: &Function,
    table: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    let Some(role @ (R::Check | R::Close)) = table
        .control_helpers
        .iter()
        .find(|helper| helper.function() == function.symbol)
        .map(|helper| helper.role())
    else {
        return Ok(());
    };
    let checked = (|| {
        if function.parameters.len() != 1 {
            return Err("lifecycle helper has invalid parameters");
        }
        let schema = lifecycle::Schema::checked(table)?;
        let definitions = crate::cc::value_identity::definitions(&function.assignments);
        let paths = paths(
            &function.assignments,
            Trace::default(),
            function.symbol,
            &definitions,
        );
        let mut coverage = std::collections::HashSet::new();
        let normal = paths
            .iter()
            .filter(|path| !path.trapped)
            .map(|path| path.events.clone())
            .collect::<Vec<_>>();
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
                    .ok_or("lifecycle has no raw contract")?
                    .function(),
            };
            if path.trapped {
                events::exceptional(
                    &view,
                    &path.events,
                    &normal,
                    &[(schema.boundary, vec![0, 1])],
                    schema.status,
                )?;
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
            coverage.insert(lifecycle::verify(
                &schema,
                &view,
                role,
                function.parameters[0],
                Some(result),
                next,
                &path.events,
            )?);
        }
        if coverage.len() != 2 {
            return Err("lifecycle must retain root and child actions");
        }
        Ok(())
    })();
    checked.map_err(|message| {
        vec![
            BackendError::invalid_ir("P8 CC lifecycle body", function.span, message)
                .with_module(function.symbol.module),
        ]
    })
}

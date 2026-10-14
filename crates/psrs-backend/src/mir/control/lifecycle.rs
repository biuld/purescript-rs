//! Recheck extent helper semantics on the physical CFG, including loopification.
use super::execution::View;
use crate::BackendError;
use crate::cc::ControlHelperRole as R;
use crate::control::execution::Operands;
use crate::control::{events, lifecycle};
use crate::mir::value_identity::{definitions, loop_header, loop_parameters};
use crate::mir::{Instruction as I, Module, Terminator as T};

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else {
        return;
    };
    let table = layout.representation_table();
    for helper in &table.control_helpers {
        let role = helper.role();
        if !matches!(role, R::Check | R::Close) {
            continue;
        }
        let Some(function) = module
            .functions
            .iter()
            .find(|function| function.symbol == helper.function())
        else {
            continue;
        };
        let checked = (|| {
            super::traces::entry(function, layout)?;
            let schema = lifecycle::Schema::checked(table)?;
            let parameters = loop_parameters(function);
            if parameters.len() != 1 {
                return Err("lifecycle helper has invalid parameters");
            }
            let definitions = definitions(function);
            let header = loop_header(function);
            let paths = crate::mir::branch_facts::paths(
                function,
                header.unwrap_or(function.entry),
                header,
            )?;
            let normal = paths
                .iter()
                .filter(|path| !path.trapped)
                .map(|path| super::traces::events(function, layout, path))
                .collect::<Result<Vec<_>, _>>()?;
            let mut coverage = std::collections::HashSet::new();
            for path in paths {
                let view = View {
                    definitions: &definitions,
                    layout,
                    aliases: &path.aliases,
                    raw: table
                        .control_resumption
                        .as_ref()
                        .ok_or("lifecycle has no raw contract")?
                        .function(),
                };
                let observed = super::traces::events(function, layout, &path)?;
                if path.trapped {
                    events::exceptional(
                        &view,
                        &observed,
                        &normal,
                        &[(schema.boundary, vec![0, 1])],
                        schema.status,
                    )?;
                    continue;
                }
                let block = function
                    .blocks
                    .iter()
                    .find(|block| block.id == path.block)
                    .ok_or("lifecycle has no exit block")?;
                let (result, next) =
                    match block.terminator.as_ref().ok_or("lifecycle has no exit")? {
                        T::Jump { arguments, .. } => (None, Some(arguments.as_slice())),
                        T::ReturnCall {
                            function: target,
                            arguments,
                            ..
                        } if *target == function.symbol => (None, Some(arguments.as_slice())),
                        T::Return { value, .. } => {
                            match definitions.get(&view.origin(*value)).copied() {
                                Some(I::Call {
                                    function: target,
                                    arguments,
                                    ..
                                }) if *target == function.symbol => {
                                    (Some(*value), Some(arguments.as_slice()))
                                }
                                _ => (Some(*value), None),
                            }
                        }
                        _ => return Err("lifecycle has an invalid exit"),
                    };
                coverage.insert(lifecycle::verify(
                    &schema,
                    &view,
                    role,
                    parameters[0],
                    result,
                    next,
                    &observed,
                )?);
            }
            if coverage.len() != 2 {
                return Err("lifecycle must retain root and child actions");
            }
            Ok(())
        })();
        if let Err(message) = checked {
            errors.push(
                BackendError::invalid_ir("P9 MIR lifecycle body", function.span, message)
                    .with_module(function.symbol.module),
            );
        }
    }
}

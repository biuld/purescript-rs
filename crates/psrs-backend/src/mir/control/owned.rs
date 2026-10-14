//! One-shot transfer evidence follows executed instructions and selected edges.
use super::execution::View;
use crate::BackendError;
use crate::cc::ControlHelperRole as R;
use crate::control::execution::{Operands, Schema};
use crate::control::owned::{self, OwnedSchema};
use crate::mir::value_identity::{definitions, loop_header, loop_parameters};
use crate::mir::{Instruction as I, Module, Terminator as T};
use std::collections::HashMap;

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else { return };
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
        let schema = Schema::checked(table)?;
        let owned = OwnedSchema::checked(table)?;
        let base = layout
            .repr_index(schema.computation)
            .map_err(|_| "owned execution has no input layout")?;
        let definitions = definitions(function);
        let header = loop_header(function);
        let blocks = function
            .blocks
            .iter()
            .map(|block| (block.id, block))
            .collect::<HashMap<_, _>>();
        let mut transfers = 0;
        for exit in
            crate::mir::branch_facts::exits(function, header.unwrap_or(function.entry), header)?
        {
            let view = View {
                definitions: &definitions,
                layout,
                aliases: &exit.aliases,
                raw: table
                    .control_resumption
                    .as_ref()
                    .ok_or("owned execution has no raw resumption contract")?
                    .function(),
            };
            if exit
                .facts
                .get(&(base, view.origin(loop_parameters(function)[2])))
                != Some(&5)
            {
                continue;
            }
            let events = super::traces::events(function, layout, &exit)?;
            let block = blocks[&exit.block];
            let next = match block
                .terminator
                .as_ref()
                .ok_or("owned execution has no exit")?
            {
                T::Jump { arguments, .. } | T::ReturnCall { arguments, .. } => arguments,
                T::Return { value, .. } => match definitions.get(&view.origin(*value)).copied() {
                    Some(I::Call {
                        function: target,
                        arguments,
                        ..
                    }) if *target == function.symbol => arguments,
                    _ => return Err("owned execution must return its executor transition"),
                },
                _ => return Err("owned execution must return its executor transition"),
            };
            owned::verify(
                &schema,
                &owned,
                &view,
                loop_parameters(function),
                next,
                &events,
            )?;
            transfers += 1;
        }
        if transfers == 0 {
            return Err("owned execution has no normal transfer path");
        }
        Ok(())
    })();
    if let Err(message) = result {
        errors.push(
            BackendError::invalid_ir("P9 MIR owned execution", function.span, message)
                .with_module(function.symbol.module),
        );
    }
}

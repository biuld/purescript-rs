//! A recursive executor call is a terminal transition, never a discarded effect.
use crate::BackendError;
use crate::cc::ControlHelperRole;
use crate::mir::value_identity::{reference_origin, step_definitions};
use crate::mir::{Function, Instruction, Module, Terminator};
use std::collections::HashSet;

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    let Some(layout) = &module.layout else { return };
    for helper in &layout.representation_table().control_helpers {
        if helper.role() != ControlHelperRole::Executor {
            continue;
        }
        let Some(function) = module
            .functions
            .iter()
            .find(|function| function.symbol == helper.function())
        else {
            continue;
        };
        if let Err(message) = verify_function(function) {
            errors.push(
                BackendError::invalid_ir("P9 MIR executor exit", function.span, message)
                    .with_module(function.symbol.module),
            );
        }
    }
}

fn verify_function(function: &Function) -> Result<(), &'static str> {
    for block in &function.blocks {
        for instruction in &block.instructions {
            let Instruction::Call {
                destination: result,
                function: callee,
                ..
            } = instruction
            else {
                continue;
            };
            if *callee != function.symbol {
                continue;
            }
            let definitions = step_definitions(function, block.id);
            let mut pending = vec![block.id];
            let mut seen = HashSet::new();
            while let Some(id) = pending.pop() {
                if !seen.insert(id) {
                    continue;
                }
                let exit = function
                    .blocks
                    .iter()
                    .find(|block| block.id == id)
                    .and_then(|block| block.terminator.as_ref())
                    .ok_or("executor transition has no actual exit")?;
                match exit {
                    Terminator::Return { value, .. }
                        if reference_origin(*value, &definitions) == *result => {}
                    Terminator::Trap { .. } => {}
                    Terminator::Return { .. }
                    | Terminator::ReturnCall { .. }
                    | Terminator::ReturnCallRef { .. } => {
                        return Err("executor transition result must reach every normal exit");
                    }
                    Terminator::Jump { target, .. }
                        if Some(*target) == crate::mir::value_identity::loop_header(function) =>
                    {
                        return Err("executor transition result must reach every normal exit");
                    }
                    terminator => pending.extend(crate::mir::cfg::successors(terminator)),
                }
            }
        }
    }
    Ok(())
}

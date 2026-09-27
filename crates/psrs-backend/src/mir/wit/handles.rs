//! Scope checks for resource handles after canonical lowering.
//!
//! The compiler does not drop handles on its own; the standard library calls
//! `resource.drop` explicitly (DEC-14). This verifier only rejects an owned
//! handle that is dropped or transferred more than once.

use super::super::{Function, Instruction};
use crate::BackendError;
use crate::abi::{self, HandleMode};
use crate::mir::BoundWasiImport;
use crate::types::ValueId;
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

/// Checks explicit `resource.drop` calls and `own` transfers in `function`.
/// `imports` is keyed by the external symbol; call instructions use the
/// registry symbol stored on [`abi::WasiImport::symbol`].
pub(in crate::mir) fn verify_function(
    function: &Function,
    imports: &HashMap<SymbolId, BoundWasiImport>,
) -> Result<(), Vec<BackendError>> {
    let mut own_arguments = HashMap::new();
    let mut drop_symbols = HashSet::new();
    for bound in imports.values() {
        // A source-declared or interned `[resource-drop]<T>` import.
        if bound.import.name.starts_with("[resource-drop]") {
            drop_symbols.insert(bound.import.symbol);
        }
        if let abi::WasiResultKind::Handle(handle) = &bound.import.result_kind {
            drop_symbols.insert(handle.drop_symbol);
        }
        let mut own = Vec::new();
        for (index, kind) in bound.import.param_kinds.iter().enumerate() {
            if let abi::WasiParamKind::Handle(handle) = kind {
                drop_symbols.insert(handle.drop_symbol);
                if handle.mode == HandleMode::Own {
                    own.push(index);
                }
            }
        }
        if !own.is_empty() {
            own_arguments.insert(bound.import.symbol, own);
        }
    }
    if drop_symbols.is_empty() && own_arguments.is_empty() {
        return Ok(());
    }
    let mut dead = HashSet::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            step_instruction(
                function,
                instruction,
                &own_arguments,
                &drop_symbols,
                &mut dead,
            )?;
        }
    }
    Ok(())
}

fn step_instruction(
    function: &Function,
    instruction: &Instruction,
    own_arguments: &HashMap<SymbolId, Vec<usize>>,
    drop_symbols: &HashSet<SymbolId>,
    dead: &mut HashSet<ValueId>,
) -> Result<(), Vec<BackendError>> {
    match instruction {
        Instruction::CallVoid {
            function: symbol,
            arguments,
            span,
        } if drop_symbols.contains(symbol) => {
            if arguments.len() == 1 {
                mark_dead(function, arguments[0], *span, dead)?;
            }
        }
        Instruction::Call {
            function: symbol,
            arguments,
            span,
            ..
        }
        | Instruction::CallVoid {
            function: symbol,
            arguments,
            span,
        } => {
            if let Some(indices) = own_arguments.get(symbol) {
                for index in indices {
                    if let Some(value) = arguments.get(*index) {
                        mark_dead(function, *value, *span, dead)?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn mark_dead(
    function: &Function,
    value: ValueId,
    span: TextRange,
    dead: &mut HashSet<ValueId>,
) -> Result<(), Vec<BackendError>> {
    if !dead.insert(value) {
        return Err(vec![BackendError::invalid_ir(
            "P9 MIR lowering",
            span,
            format!(
                "function `{}` drops or transfers an owned handle twice",
                function.name
            ),
        )]);
    }
    Ok(())
}

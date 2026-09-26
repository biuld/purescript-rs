//! Scope rules for resource handles after canonical lowering.
//!
//! An `own<T>` index may be dropped once, or transferred once into an `own`
//! parameter. A `borrow<T>` index may be used only until `resource.drop`
//! releases it at the end of the call that created it. A second drop of an
//! owned handle, or any use of a borrow after that release, is rejected.

use super::super::{Function, Instruction, Terminator};
use crate::BackendError;
use crate::abi::{self, BoundWasiImport, HandleMode};
use crate::types::ValueId;
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Life {
    LiveOwn,
    LiveBorrow,
    /// Dropped or transferred. A later `resource.drop` is a double drop.
    DeadOwn,
    /// Released at the end of its call. A later use is outside the borrow.
    DeadBorrow,
}

/// Checks handle introductions, transfers, and `resource.drop` calls in
/// `function`. `imports` is keyed by the external symbol; call instructions
/// use the registry symbol stored on [`abi::WasiImport::symbol`].
pub(in crate::mir) fn verify_function(
    function: &Function,
    imports: &HashMap<SymbolId, BoundWasiImport>,
) -> Result<(), Vec<BackendError>> {
    let mut introductions = HashMap::new();
    let mut own_arguments = HashMap::new();
    let mut drop_symbols = HashSet::new();
    for bound in imports.values() {
        if let abi::WasiResultKind::Handle(handle) = &bound.import.result_kind {
            introductions.insert(bound.import.symbol, handle.mode);
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
    if introductions.is_empty() && drop_symbols.is_empty() {
        return Ok(());
    }
    let mut life = HashMap::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            step_instruction(
                function,
                instruction,
                &introductions,
                &own_arguments,
                &drop_symbols,
                &mut life,
            )?;
        }
        if let Some(terminator) = &block.terminator {
            let span = terminator_span(terminator);
            for value in terminator_operands(terminator) {
                check_use(function, value, span, &life)?;
            }
        }
    }
    Ok(())
}

fn step_instruction(
    function: &Function,
    instruction: &Instruction,
    introductions: &HashMap<SymbolId, HandleMode>,
    own_arguments: &HashMap<SymbolId, Vec<usize>>,
    drop_symbols: &HashSet<SymbolId>,
    life: &mut HashMap<ValueId, Life>,
) -> Result<(), Vec<BackendError>> {
    if let Instruction::CallVoid {
        function: symbol,
        arguments,
        span,
    } = instruction
        && drop_symbols.contains(symbol)
    {
        if arguments.len() == 1 {
            drop_handle(function, arguments[0], *span, life)?;
        }
        return Ok(());
    }
    let use_span = instruction.span();
    for value in instruction.operands() {
        check_use(function, value, use_span, life)?;
    }
    match instruction {
        Instruction::Call {
            destination,
            function: symbol,
            arguments,
            span,
        } => {
            transfer(function, *symbol, arguments, *span, own_arguments, life)?;
            if let Some(mode) = introductions.get(symbol) {
                life.insert(
                    *destination,
                    match mode {
                        HandleMode::Own => Life::LiveOwn,
                        HandleMode::Borrow => Life::LiveBorrow,
                    },
                );
            }
        }
        Instruction::CallVoid {
            function: symbol,
            arguments,
            span,
        } => transfer(function, *symbol, arguments, *span, own_arguments, life)?,
        _ => {}
    }
    Ok(())
}

fn transfer(
    function: &Function,
    symbol: SymbolId,
    arguments: &[ValueId],
    span: TextRange,
    own_arguments: &HashMap<SymbolId, Vec<usize>>,
    life: &mut HashMap<ValueId, Life>,
) -> Result<(), Vec<BackendError>> {
    let Some(indices) = own_arguments.get(&symbol) else {
        return Ok(());
    };
    for index in indices {
        let Some(value) = arguments.get(*index) else {
            continue;
        };
        match life.get(value).copied() {
            Some(Life::LiveOwn) => {
                life.insert(*value, Life::DeadOwn);
            }
            Some(Life::DeadOwn) => {
                return Err(dropped_twice(function, span));
            }
            Some(Life::LiveBorrow | Life::DeadBorrow) | None => {}
        }
    }
    Ok(())
}

fn drop_handle(
    function: &Function,
    value: ValueId,
    span: TextRange,
    life: &mut HashMap<ValueId, Life>,
) -> Result<(), Vec<BackendError>> {
    match life.get(&value).copied() {
        Some(Life::LiveOwn) => {
            life.insert(value, Life::DeadOwn);
            Ok(())
        }
        Some(Life::LiveBorrow) => {
            life.insert(value, Life::DeadBorrow);
            Ok(())
        }
        Some(Life::DeadOwn) => Err(dropped_twice(function, span)),
        Some(Life::DeadBorrow) => Err(after_borrow(function, span)),
        None => Ok(()),
    }
}

fn check_use(
    function: &Function,
    value: ValueId,
    span: TextRange,
    life: &HashMap<ValueId, Life>,
) -> Result<(), Vec<BackendError>> {
    if life.get(&value) == Some(&Life::DeadBorrow) {
        return Err(after_borrow(function, span));
    }
    Ok(())
}

fn dropped_twice(function: &Function, span: TextRange) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P9 MIR lowering",
        span,
        format!(
            "function `{}` has an owned handle dropped twice",
            function.name
        ),
    )]
}

fn after_borrow(function: &Function, span: TextRange) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P9 MIR lowering",
        span,
        format!(
            "function `{}` uses a handle after its borrow scope",
            function.name
        ),
    )]
}

fn terminator_operands(terminator: &Terminator) -> Vec<ValueId> {
    match terminator {
        Terminator::Return { value, .. } => vec![*value],
        Terminator::Jump { arguments, .. } => arguments.clone(),
        Terminator::Branch { condition, .. } => vec![*condition],
        Terminator::Switch { value, .. } => vec![*value],
        Terminator::ReturnCall { arguments, .. } => arguments.clone(),
        Terminator::ReturnCallRef {
            function,
            arguments,
            ..
        } => std::iter::once(*function)
            .chain(arguments.iter().copied())
            .collect(),
    }
}

/// An owned handle still waiting for `resource.drop` or an `own` transfer.
pub(in crate::mir) struct OwnedObligation {
    pub value: ValueId,
    pub drop_symbol: SymbolId,
    pub span: TextRange,
}

/// Emits `resource.drop` for owned handles that this function did not return
/// and did not transfer. The drop runs in `block` before that block's return.
pub(in crate::mir) fn owned_drops(
    obligations: &[OwnedObligation],
    returned: ValueId,
) -> Vec<Instruction> {
    obligations
        .iter()
        .filter(|obligation| obligation.value != returned)
        .map(|obligation| Instruction::CallVoid {
            function: obligation.drop_symbol,
            arguments: vec![obligation.value],
            span: obligation.span,
        })
        .collect()
}

/// `resource.drop` of a borrow result. The borrow's scope is the call that
/// produced it, so the release is the next instruction after that call.
pub(super) fn borrow_release(
    handle: ValueId,
    drop_symbol: SymbolId,
    span: TextRange,
) -> Instruction {
    Instruction::CallVoid {
        function: drop_symbol,
        arguments: vec![handle],
        span,
    }
}

fn terminator_span(terminator: &Terminator) -> TextRange {
    match terminator {
        Terminator::Return { span, .. }
        | Terminator::Jump { span, .. }
        | Terminator::Branch { span, .. }
        | Terminator::Switch { span, .. }
        | Terminator::ReturnCall { span, .. }
        | Terminator::ReturnCallRef { span, .. } => *span,
    }
}

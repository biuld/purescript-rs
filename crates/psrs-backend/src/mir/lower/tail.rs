//! Tail-call analysis and self-recursion loopification.
//!
//! P9 lowers every call to an instruction followed by an ordinary return
//! (possibly through a one-parameter join). This pass recognizes calls whose
//! result is returned unchanged on all paths:
//!
//! - a self call becomes a back edge to a fresh loop header carrying the
//!   function parameters as block parameters, so deep self-recursion runs in
//!   constant stack on every target profile;
//! - a non-self direct or reference call becomes `ReturnCall`/`ReturnCallRef`
//!   when the target enables the tail-call proposal, and is otherwise left as
//!   an ordinary call plus return.

use super::{BasicBlock, BlockId, Function, Terminator};
use crate::BackendError;
use crate::capability::TargetCapabilities;
use crate::mir::Instruction;
use crate::types::{DefinedTypeId, HeapType, RefType, ValueDecl, ValueId, ValueType};
use std::collections::{HashMap, HashSet};

pub(super) fn mark_tail(
    function: &mut Function,
    target: TargetCapabilities,
) -> Result<(), Vec<BackendError>> {
    let returned = return_forwarded_values(function);
    let uses = use_counts(function);
    let symbol = function.symbol;

    let mut self_calls: Vec<(BlockId, Vec<ValueId>, psrs_span::TextRange)> = Vec::new();
    let mut non_self: Vec<(BlockId, NonSelfCall, Vec<ValueId>, psrs_span::TextRange)> = Vec::new();
    let mut closure_calls: Vec<ClosureTail> = Vec::new();

    for block in &function.blocks {
        let Some(last) = block.instructions.last() else {
            continue;
        };
        let destination = match last {
            Instruction::Call { destination, .. }
            | Instruction::CallRef { destination, .. }
            | Instruction::ClosureCall { destination, .. } => *destination,
            _ => continue,
        };
        if !returned.contains(&destination) {
            continue;
        }
        if uses.get(&destination).copied().unwrap_or(0) != 1 {
            continue;
        }
        match last {
            Instruction::Call {
                function: callee,
                arguments,
                span,
                ..
            } if *callee == symbol => {
                self_calls.push((block.id, arguments.clone(), *span));
            }
            Instruction::Call {
                function: callee,
                arguments,
                span,
                ..
            } => non_self.push((
                block.id,
                NonSelfCall::Direct(*callee),
                arguments.clone(),
                *span,
            )),
            Instruction::CallRef {
                function: callee,
                arguments,
                span,
                ..
            } => non_self.push((
                block.id,
                NonSelfCall::Reference(*callee),
                arguments.clone(),
                *span,
            )),
            Instruction::ClosureCall {
                function: callee,
                type_index,
                closure_type,
                arguments,
                span,
                ..
            } => closure_calls.push(ClosureTail {
                block: block.id,
                function: *callee,
                type_index: *type_index,
                closure_type: *closure_type,
                arguments: arguments.clone(),
                span: *span,
            }),
            _ => {}
        }
    }

    if target.tail_call {
        for (block_id, call, arguments, span) in non_self {
            let terminator = match call {
                NonSelfCall::Direct(callee) => Terminator::ReturnCall {
                    function: callee,
                    arguments,
                    span,
                },
                NonSelfCall::Reference(callee) => Terminator::ReturnCallRef {
                    function: callee,
                    arguments,
                    span,
                },
            };
            replace_tail_call(function, block_id, terminator)?;
        }
        for call in closure_calls {
            rewrite_closure_tail_call(function, call)?;
        }
    }

    loopify_self_calls(function, &self_calls)?;
    Ok(())
}

enum NonSelfCall {
    Direct(psrs_hir::SymbolId),
    Reference(ValueId),
}

struct ClosureTail {
    block: BlockId,
    function: ValueId,
    type_index: DefinedTypeId,
    closure_type: DefinedTypeId,
    arguments: Vec<ValueId>,
    span: psrs_span::TextRange,
}

/// Rewrites a closure tail call into `ReturnCallRef`: project the closure's
/// code reference, cast it to the call signature, and pass the closure as the
/// receiver argument.
fn rewrite_closure_tail_call(
    function: &mut Function,
    call: ClosureTail,
) -> Result<(), Vec<BackendError>> {
    let span = call.span;
    let next = function
        .values
        .iter()
        .map(|value| value.id.0)
        .max()
        .map_or(0, |max| max + 1);
    let code_raw = ValueId(next);
    let code = ValueId(next + 1);
    function.values.push(ValueDecl {
        id: code_raw,
        ty: ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Func,
        }),
    });
    function.values.push(ValueDecl {
        id: code,
        ty: ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(call.type_index),
        }),
    });
    let block = function
        .blocks
        .iter_mut()
        .find(|block| block.id == call.block)
        .ok_or_else(|| invalid_ir(span, "closure tail-call block does not exist"))?;
    let _ = block.instructions.pop();
    block.instructions.push(Instruction::StructGet {
        destination: code_raw,
        type_index: call.closure_type,
        field: 0,
        value: call.function,
        span,
    });
    block.instructions.push(Instruction::RefCast {
        destination: code,
        value: code_raw,
        reference: RefType {
            nullable: false,
            heap: HeapType::Index(call.type_index),
        },
        span,
    });
    let mut arguments = Vec::with_capacity(call.arguments.len() + 1);
    arguments.push(call.function);
    arguments.extend(call.arguments);
    block.terminator = Some(Terminator::ReturnCallRef {
        function: code,
        arguments,
        span,
    });
    Ok(())
}

/// Replaces the trailing tail-call instruction with a terminator.
fn replace_tail_call(
    function: &mut Function,
    block_id: BlockId,
    terminator: Terminator,
) -> Result<(), Vec<BackendError>> {
    let span = function.span;
    let block = function
        .blocks
        .iter_mut()
        .find(|block| block.id == block_id)
        .ok_or_else(|| invalid_ir(span, "tail-call block does not exist"))?;
    let _ = block.instructions.pop();
    block.terminator = Some(terminator);
    Ok(())
}

/// Rewrites every self tail call into a jump to a fresh loop header. The
/// original entry becomes a preheader that passes the function parameters to
/// the header, whose block parameters replace uses of the function parameters
/// in the loop body.
fn loopify_self_calls(
    function: &mut Function,
    self_calls: &[(BlockId, Vec<ValueId>, psrs_span::TextRange)],
) -> Result<(), Vec<BackendError>> {
    if self_calls.is_empty() {
        return Ok(());
    }
    let span = function.span;
    let entry = function.entry;
    let next_value = function
        .values
        .iter()
        .map(|value| value.id.0)
        .max()
        .map_or(0, |max| max + 1);
    let parameter_types = function
        .parameters
        .iter()
        .map(|parameter| {
            function
                .values
                .iter()
                .find(|value| value.id == *parameter)
                .map(|value| value.ty)
                .ok_or_else(|| invalid_ir(span, "function parameter has no type"))
        })
        .collect::<Result<Vec<ValueType>, _>>()?;
    let mut header_parameters = Vec::with_capacity(parameter_types.len());
    for (offset, ty) in parameter_types.into_iter().enumerate() {
        let id = ValueId(next_value + offset as u32);
        function.values.push(ValueDecl { id, ty });
        header_parameters.push(id);
    }
    let header_id = BlockId(
        function
            .blocks
            .iter()
            .map(|block| block.id.0)
            .max()
            .map_or(0, |max| max + 1),
    );
    let entry_index = function
        .blocks
        .iter()
        .position(|block| block.id == entry)
        .ok_or_else(|| invalid_ir(span, "function entry block does not exist"))?;
    let instructions = std::mem::take(&mut function.blocks[entry_index].instructions);
    let terminator = function.blocks[entry_index].terminator.take();
    function.blocks.push(BasicBlock {
        id: header_id,
        parameters: header_parameters.clone(),
        instructions,
        terminator,
    });
    function.blocks[entry_index].terminator = Some(Terminator::Jump {
        target: header_id,
        arguments: function.parameters.clone(),
        span: function.span,
    });

    let mapping = function
        .parameters
        .iter()
        .cloned()
        .zip(header_parameters.iter().cloned())
        .collect::<HashMap<_, _>>();
    for block in &mut function.blocks {
        if block.id == entry {
            continue;
        }
        for instruction in &mut block.instructions {
            crate::mir::opt::remap_instruction(instruction, &mapping);
        }
        if let Some(terminator) = &mut block.terminator {
            crate::mir::opt::remap_terminator(terminator, &mapping);
        }
    }

    for (block_id, arguments, span) in self_calls {
        let target_block = if *block_id == entry {
            header_id
        } else {
            *block_id
        };
        let block = function
            .blocks
            .iter_mut()
            .find(|block| block.id == target_block)
            .ok_or_else(|| invalid_ir(function.span, "self tail-call block does not exist"))?;
        block.instructions.pop();
        block.terminator = Some(Terminator::Jump {
            target: header_id,
            arguments: arguments.clone(),
            span: *span,
        });
    }
    Ok(())
}

/// Values that are returned unchanged, either directly by a `Return` or passed
/// through one-parameter joins to a returned block parameter.
fn return_forwarded_values(function: &Function) -> HashSet<ValueId> {
    let mut returned = HashSet::new();
    for block in &function.blocks {
        if let Some(Terminator::Return { value, .. }) = &block.terminator {
            returned.insert(*value);
        }
    }
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    loop {
        let mut changed = false;
        for block in &function.blocks {
            let Some(Terminator::Jump {
                target, arguments, ..
            }) = &block.terminator
            else {
                continue;
            };
            let Some(target) = blocks.get(target) else {
                continue;
            };
            for (index, parameter) in target.parameters.iter().enumerate() {
                if returned.contains(parameter)
                    && let Some(argument) = arguments.get(index)
                {
                    changed |= returned.insert(*argument);
                }
            }
        }
        if !changed {
            break;
        }
    }
    returned
}

fn use_counts(function: &Function) -> HashMap<ValueId, usize> {
    let mut counts = HashMap::new();
    let mut record = |value: ValueId| {
        *counts.entry(value).or_insert(0) += 1;
    };
    for block in &function.blocks {
        for instruction in &block.instructions {
            for operand in instruction.operands() {
                record(operand);
            }
        }
        if let Some(terminator) = &block.terminator {
            for operand in terminator_operands(terminator) {
                record(operand);
            }
        }
    }
    counts
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

fn invalid_ir(span: psrs_span::TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::invalid_ir("P9 MIR lowering", span, message)]
}

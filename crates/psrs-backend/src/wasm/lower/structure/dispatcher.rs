use super::super::{local, wasm_error};
use super::Structurer;
use crate::BackendError;
use crate::mir::{BasicBlock, BlockId, Terminator};
use crate::types::ValueId;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use std::collections::HashMap;
use wasm_encoder::Instruction;

pub(super) trait DispatcherOps {
    fn emit_dispatcher(&self, blocks: &[BlockId], body: &mut Body)
    -> Result<(), Vec<BackendError>>;
}

struct DispatcherState<'a> {
    local: u32,
    indices: &'a HashMap<BlockId, i32>,
}

impl DispatcherOps for Structurer<'_> {
    fn emit_dispatcher(
        &self,
        block_ids: &[BlockId],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        let count = i32::try_from(block_ids.len())
            .map_err(|_| wasm_error(self.function.span, "MIR has too many blocks for dispatch"))?;
        let list_locals = u32::from(self.list_locals.is_some()) * 2;
        let state_local = u32::try_from(self.function.values.len())
            .map_err(|_| wasm_error(self.function.span, "MIR function has too many values"))?
            + list_locals;
        let state_indices = block_ids
            .iter()
            .enumerate()
            .map(|(index, block)| (*block, index as i32))
            .collect::<HashMap<_, _>>();
        let entry_state = state_index(&state_indices, self.function.entry, self.function.span)?;
        let state = DispatcherState {
            local: state_local,
            indices: &state_indices,
        };

        body.push(Op::Leaf(Instruction::I32Const(entry_state)));
        body.push(Op::Leaf(Instruction::LocalSet(state.local)));

        let table_targets = (0..count as u32)
            .map(|index| count as u32 - index - 1)
            .collect();
        let mut dispatch = vec![
            Op::Leaf(Instruction::LocalGet(state.local)),
            Op::Leaf(Instruction::BrTable(table_targets, count as u32)),
        ];

        for index in (0..block_ids.len()).rev() {
            let block_id = block_ids[index];
            let span = block_span(self.blocks.get(&block_id).copied(), self.function.span);
            let mut sequence = vec![Op::Block {
                body: dispatch,
                result: None,
                span,
            }];
            self.emit_dispatched_block(block_id, index, &state, &mut sequence)?;
            dispatch = sequence;
        }

        let loop_body = vec![
            Op::Block {
                body: dispatch,
                result: None,
                span: self.function.span,
            },
            Op::Leaf(Instruction::Unreachable),
        ];
        let loop_span = self
            .blocks
            .get(&self.function.entry)
            .copied()
            .map_or(self.function.span, |block| {
                block_span(Some(block), self.function.span)
            });
        body.push(Op::Loop {
            body: loop_body,
            result: None,
            span: loop_span,
        });
        Ok(())
    }
}

impl Structurer<'_> {
    fn emit_dispatched_block(
        &self,
        block_id: BlockId,
        index: usize,
        state: &DispatcherState<'_>,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        let block = self
            .blocks
            .get(&block_id)
            .copied()
            .ok_or_else(|| wasm_error(self.function.span, "MIR block does not exist"))?;
        self.emit_block_instructions(&block.instructions, body)?;
        let terminator = block
            .terminator
            .as_ref()
            .ok_or_else(|| wasm_error(self.function.span, "MIR block has no terminator"))?;
        let span = terminator_span(terminator);
        match terminator {
            Terminator::Return { value, .. } => {
                self.emit_load(*value, span, body)?;
                body.push(Op::Leaf(Instruction::Return));
                return Ok(());
            }
            Terminator::Jump {
                target, arguments, ..
            } => {
                self.emit_jump_arguments(*target, arguments, span, body)?;
                self.emit_state_update(*target, state, span, body)?;
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
                ..
            } => {
                self.require_parameterless_target(*then_block, span)?;
                self.require_parameterless_target(*else_block, span)?;
                self.emit_load(*condition, span, body)?;
                body.push(Op::If {
                    then_body: self.state_update(*then_block, state, span)?,
                    else_body: self.state_update(*else_block, state, span)?,
                    result: None,
                    span,
                });
            }
            Terminator::Switch {
                value,
                cases,
                default,
                ..
            } => self.emit_switch_state(*value, cases, *default, state, span, body)?,
            Terminator::ReturnCall {
                function,
                arguments,
                ..
            } => {
                self.emit_return_call(*function, arguments, span, body)?;
                return Ok(());
            }
            Terminator::ReturnCallRef {
                function,
                arguments,
                ..
            } => {
                self.emit_return_call_ref(*function, arguments, span, body)?;
                return Ok(());
            }
        }
        body.push(Op::Leaf(Instruction::Br(index as u32 + 1)));
        Ok(())
    }

    fn emit_switch_state(
        &self,
        selector: ValueId,
        cases: &[(i32, BlockId)],
        default: BlockId,
        state: &DispatcherState<'_>,
        span: TextRange,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        self.require_parameterless_target(default, span)?;
        let mut chain = self.state_update(default, state, span)?;
        for (tag, target) in cases.iter().rev() {
            self.require_parameterless_target(*target, span)?;
            chain = vec![
                Op::Leaf(Instruction::LocalGet(local(&self.locals, selector, span)?)),
                Op::Leaf(Instruction::I32Const(*tag)),
                Op::Leaf(Instruction::I32Eq),
                Op::If {
                    then_body: self.state_update(*target, state, span)?,
                    else_body: chain,
                    result: None,
                    span,
                },
            ];
        }
        body.extend(chain);
        Ok(())
    }

    fn state_update(
        &self,
        target: BlockId,
        state: &DispatcherState<'_>,
        span: TextRange,
    ) -> Result<Body, Vec<BackendError>> {
        Ok(vec![
            Op::Leaf(Instruction::I32Const(state_index(
                state.indices,
                target,
                span,
            )?)),
            Op::Leaf(Instruction::LocalSet(state.local)),
        ])
    }

    fn emit_state_update(
        &self,
        target: BlockId,
        state: &DispatcherState<'_>,
        span: TextRange,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        body.extend(self.state_update(target, state, span)?);
        Ok(())
    }
}

fn state_index(
    indices: &HashMap<BlockId, i32>,
    block: BlockId,
    span: TextRange,
) -> Result<i32, Vec<BackendError>> {
    indices
        .get(&block)
        .copied()
        .ok_or_else(|| wasm_error(span, "MIR branch target is not reachable from the entry"))
}

fn block_span(block: Option<&BasicBlock>, fallback: TextRange) -> TextRange {
    block
        .and_then(|block| block.terminator.as_ref())
        .map_or(fallback, terminator_span)
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

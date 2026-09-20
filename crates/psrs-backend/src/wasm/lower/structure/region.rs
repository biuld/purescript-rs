use super::{Structurer, local, value_type, wasm_error};
use crate::BackendError;
use crate::mir::{BlockId, Terminator};
use crate::types::ValueId;
use crate::wasm::convert::val_type;
use crate::wasm::{Body, Op};
use std::collections::HashSet;
use wasm_encoder::Instruction;

pub(super) trait RegionOps {
    fn emit_region(
        &self,
        current: BlockId,
        stop: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
        body: &mut Body,
    ) -> Result<Option<ValueId>, Vec<BackendError>>;
}

impl RegionOps for Structurer<'_> {
    fn emit_region(
        &self,
        mut current: BlockId,
        stop: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
        body: &mut Body,
    ) -> Result<Option<ValueId>, Vec<BackendError>> {
        loop {
            if stop == Some(current) {
                let block = self.blocks.get(&current).ok_or_else(|| {
                    wasm_error(self.function.span, "MIR branch merge block is missing")
                })?;
                return Ok(block.parameters.first().copied());
            }
            if !visited.insert(current) {
                return Err(wasm_error(
                    self.function.span,
                    "MIR contains a loop that the first Wasm structurer cannot lower",
                ));
            }
            let block = self
                .blocks
                .get(&current)
                .ok_or_else(|| wasm_error(self.function.span, "MIR block does not exist"))?;
            self.emit_block_instructions(&block.instructions, body)?;
            let Some(terminator) = &block.terminator else {
                return Err(wasm_error(
                    self.function.span,
                    "MIR block has no terminator",
                ));
            };
            match terminator {
                Terminator::Return { .. } => return Ok(None),
                Terminator::Jump {
                    target,
                    arguments,
                    span,
                } => {
                    if stop == Some(*target) {
                        if arguments.len() != 1 {
                            return Err(wasm_error(
                                self.function.span,
                                "structured branch must pass one result value to its merge",
                            ));
                        }
                        return Ok(arguments.first().copied());
                    }
                    let target_block = self.blocks.get(target).ok_or_else(|| {
                        wasm_error(self.function.span, "MIR jump target is missing")
                    })?;
                    if target_block.parameters.len() != arguments.len() {
                        return Err(wasm_error(
                            self.function.span,
                            "MIR jump argument count differs from block parameters",
                        ));
                    }
                    for (argument, parameter) in
                        arguments.iter().zip(&target_block.parameters).rev()
                    {
                        body.push(Op::Leaf(Instruction::LocalGet(local(
                            &self.locals,
                            *argument,
                            *span,
                        )?)));
                        body.push(Op::Leaf(Instruction::LocalSet(local(
                            &self.locals,
                            *parameter,
                            *span,
                        )?)));
                    }
                    current = *target;
                }
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                    merge_block,
                    span,
                } => {
                    let merge = self
                        .blocks
                        .get(merge_block)
                        .ok_or_else(|| wasm_error(*span, "MIR merge block is missing"))?;
                    if merge.parameters.len() != 1 {
                        return Err(wasm_error(
                            *span,
                            "MIR branch merge must have one result value",
                        ));
                    }
                    let merge_value = merge.parameters[0];
                    let merge_type = value_type(self.function, merge_value).ok_or_else(|| {
                        wasm_error(*span, "MIR merge parameter has no value type")
                    })?;
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *condition,
                        *span,
                    )?)));
                    let mut then_body = Body::new();
                    let then_value = self
                        .emit_region(*then_block, Some(*merge_block), visited, &mut then_body)?
                        .ok_or_else(|| {
                            wasm_error(self.function.span, "then arm does not reach its merge")
                        })?;
                    then_body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        then_value,
                        *span,
                    )?)));
                    let mut else_body = Body::new();
                    let else_value = self
                        .emit_region(*else_block, Some(*merge_block), visited, &mut else_body)?
                        .ok_or_else(|| {
                            wasm_error(self.function.span, "else arm does not reach its merge")
                        })?;
                    else_body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        else_value,
                        *span,
                    )?)));
                    body.push(Op::If {
                        then_body,
                        else_body,
                        result: Some(val_type(merge_type)),
                        span: *span,
                    });
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        merge_value,
                        *span,
                    )?)));
                    current = *merge_block;
                }
            }
        }
    }
}

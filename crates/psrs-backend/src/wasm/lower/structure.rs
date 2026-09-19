use super::{local, wasm_error};
use crate::BackendError;
use crate::mir::{
    self, BlockId, Function as MirFunction, Instruction as MirInstruction, Terminator,
};
use crate::types::ValueId;
use crate::wasm::{Body, Op};
use psrs_core::Primitive;
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};
use wasm_encoder::{Instruction, ValType};

pub(super) struct Structurer<'a> {
    pub(super) function: &'a MirFunction,
    pub(super) blocks: HashMap<BlockId, &'a mir::BasicBlock>,
    pub(super) locals: HashMap<ValueId, u32>,
    pub(super) function_indices: &'a HashMap<SymbolId, u32>,
    pub(super) string_offsets: &'a HashMap<String, u32>,
}

impl Structurer<'_> {
    pub(super) fn emit_region(
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
                        result: Some(ValType::I32),
                        span: *span,
                    });
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
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        merge.parameters[0],
                        *span,
                    )?)));
                    current = *merge_block;
                }
            }
        }
    }

    fn emit_block_instructions(
        &self,
        instructions: &[MirInstruction],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        for instruction in instructions {
            match instruction {
                MirInstruction::Constant {
                    destination,
                    value,
                    span,
                } => {
                    body.push(Op::Leaf(Instruction::I32Const(*value)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::StringConstant {
                    destination,
                    bytes,
                    span,
                } => {
                    let offset =
                        self.string_offsets.get(bytes).copied().ok_or_else(|| {
                            wasm_error(*span, "string constant has no data segment")
                        })?;
                    body.push(Op::Leaf(Instruction::I32Const(offset as i32)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::Copy {
                    destination,
                    value,
                    span,
                } => {
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *value,
                        *span,
                    )?)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::Primitive {
                    destination,
                    op,
                    left,
                    right,
                    span,
                } => {
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *left,
                        *span,
                    )?)));
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *right,
                        *span,
                    )?)));
                    body.push(Op::Leaf(primitive(*op)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
                MirInstruction::Call {
                    destination,
                    function,
                    arguments,
                    span,
                } => {
                    for argument in arguments {
                        body.push(Op::Leaf(Instruction::LocalGet(local(
                            &self.locals,
                            *argument,
                            *span,
                        )?)));
                    }
                    let index = self
                        .function_indices
                        .get(function)
                        .copied()
                        .ok_or_else(|| {
                            wasm_error(*span, "MIR call target has no Wasm function index")
                        })?;
                    body.push(Op::Leaf(Instruction::Call(index)));
                    body.push(Op::Leaf(Instruction::LocalSet(local(
                        &self.locals,
                        *destination,
                        *span,
                    )?)));
                }
            }
        }
        Ok(())
    }
}

fn primitive(op: Primitive) -> Instruction<'static> {
    match op {
        Primitive::Add => Instruction::I32Add,
        Primitive::Sub => Instruction::I32Sub,
        Primitive::Mul => Instruction::I32Mul,
        Primitive::DivS => Instruction::I32DivS,
        Primitive::RemS => Instruction::I32RemS,
        Primitive::Eq => Instruction::I32Eq,
        Primitive::Ne => Instruction::I32Ne,
        Primitive::LtS => Instruction::I32LtS,
        Primitive::LeS => Instruction::I32LeS,
        Primitive::GtS => Instruction::I32GtS,
        Primitive::GeS => Instruction::I32GeS,
    }
}

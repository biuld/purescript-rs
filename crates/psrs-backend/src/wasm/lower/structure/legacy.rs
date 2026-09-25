use super::{Structurer, block_traps, local, value_type, wasm_error};
use crate::BackendError;
use crate::mir::{BlockId, Terminator};
use crate::types::ValueId;
use crate::wasm::convert::val_type;
use crate::wasm::{Body, Op};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use wasm_encoder::{Instruction, ValType};

/// How a structured region handed control back to its caller. `Value` carries
/// the one result the region produced; `Terminated` means the region already
/// returned or trapped, so no value flows and no `local.get` may be emitted.
pub(super) enum RegionExit {
    Value(ValueId),
    Terminated,
}

pub(super) trait LegacyRegionOps {
    fn emit_linear_region(
        &self,
        current: BlockId,
        stop: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
        body: &mut Body,
    ) -> Result<RegionExit, Vec<BackendError>>;

    fn emit_switch(
        &self,
        value: ValueId,
        cases: &[(i32, BlockId)],
        default: BlockId,
        span: psrs_span::TextRange,
        visited: &mut HashSet<BlockId>,
        body: &mut Body,
    ) -> Result<BlockId, Vec<BackendError>>;
}

impl LegacyRegionOps for Structurer<'_> {
    fn emit_linear_region(
        &self,
        mut current: BlockId,
        stop: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
        body: &mut Body,
    ) -> Result<RegionExit, Vec<BackendError>> {
        loop {
            if stop == Some(current) {
                let block = self.blocks.get(&current).ok_or_else(|| {
                    wasm_error(self.function.span, "MIR branch merge block is missing")
                })?;
                return Ok(block
                    .parameters
                    .first()
                    .copied()
                    .map_or(RegionExit::Terminated, RegionExit::Value));
            }
            if !visited.insert(current) {
                return Err(wasm_error(
                    self.function.span,
                    "MIR loop reached the acyclic region structurer",
                ));
            }
            let block = self
                .blocks
                .get(&current)
                .ok_or_else(|| wasm_error(self.function.span, "MIR block does not exist"))?;
            self.emit_block_instructions(&block.instructions, body)?;
            if block_traps(block) {
                return Ok(RegionExit::Terminated);
            }
            let Some(terminator) = &block.terminator else {
                return Err(wasm_error(
                    self.function.span,
                    "MIR block has no terminator",
                ));
            };
            match terminator {
                Terminator::Return { value, span } => {
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *value,
                        *span,
                    )?)));
                    body.push(Op::Leaf(Instruction::Return));
                    return Ok(RegionExit::Terminated);
                }
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
                        return Ok(RegionExit::Value(arguments[0]));
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
                    span,
                } => {
                    // The branch carries no merge hint. Derive the join as the
                    // nearest common one-value descendant of both arms; the
                    // structured `if` produces that value and control resumes
                    // at the join.
                    let join_block = crate::mir::cfg::common_join(
                        &self.function.blocks,
                        &[*then_block, *else_block],
                    )
                    .ok_or_else(|| {
                        wasm_error(*span, "MIR branch arms have no common one-value join")
                    })?;
                    let merge = self
                        .blocks
                        .get(&join_block)
                        .ok_or_else(|| wasm_error(*span, "MIR branch join is missing"))?;
                    if merge.parameters.len() != 1 {
                        return Err(wasm_error(
                            *span,
                            "MIR branch join must have one result value",
                        ));
                    }
                    let merge_value = merge.parameters[0];
                    let merge_type = value_type(self.function, merge_value).ok_or_else(|| {
                        wasm_error(*span, "MIR branch join parameter has no value type")
                    })?;
                    body.push(Op::Leaf(Instruction::LocalGet(local(
                        &self.locals,
                        *condition,
                        *span,
                    )?)));
                    let mut then_body = Body::new();
                    let then_value = self.emit_linear_region(
                        *then_block,
                        Some(join_block),
                        visited,
                        &mut then_body,
                    )?;
                    if let RegionExit::Value(value) = then_value {
                        then_body.push(Op::Leaf(Instruction::LocalGet(local(
                            &self.locals,
                            value,
                            *span,
                        )?)));
                    }
                    let mut else_body = Body::new();
                    let else_value = self.emit_linear_region(
                        *else_block,
                        Some(join_block),
                        visited,
                        &mut else_body,
                    )?;
                    if let RegionExit::Value(value) = else_value {
                        else_body.push(Op::Leaf(Instruction::LocalGet(local(
                            &self.locals,
                            value,
                            *span,
                        )?)));
                    }
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
                    current = join_block;
                }
                Terminator::Switch {
                    value,
                    cases,
                    default,
                    span,
                } => {
                    current = self.emit_switch(*value, cases, *default, *span, visited, body)?;
                }
            }
        }
    }

    fn emit_switch(
        &self,
        value: ValueId,
        cases: &[(i32, crate::mir::BlockId)],
        default: crate::mir::BlockId,
        span: psrs_span::TextRange,
        visited: &mut HashSet<crate::mir::BlockId>,
        body: &mut Body,
    ) -> Result<crate::mir::BlockId, Vec<BackendError>> {
        let targets = cases
            .iter()
            .map(|(_, target)| *target)
            .chain(std::iter::once(default))
            .collect::<Vec<_>>();
        let join = crate::mir::cfg::common_join(&self.function.blocks, &targets)
            .ok_or_else(|| wasm_error(span, "MIR switch arms have no common one-value join"))?;
        if targets.contains(&join) {
            return Err(wasm_error(
                span,
                "MIR switch successor cannot be its value-bearing join",
            ));
        }
        let join_block = self
            .blocks
            .get(&join)
            .ok_or_else(|| wasm_error(span, "MIR switch join block is missing"))?;
        if join_block.parameters.len() != 1 {
            return Err(wasm_error(
                span,
                "MIR switch join must have one result parameter",
            ));
        }
        let join_value = join_block.parameters[0];
        value_type(self.function, join_value)
            .ok_or_else(|| wasm_error(span, "MIR switch result has no value type"))?;
        let case_count = cases.len() as u32;
        let mut dispatch = vec![switch_index(value, cases, &self.locals, span)?];
        dispatch.push(Op::Leaf(Instruction::BrTable(
            Cow::Owned((0..case_count).collect()),
            case_count,
        )));

        // A WebAssembly br_table selects by dense unsigned index, while MIR
        // tags are arbitrary i32 values. Translate tags to dense arm indices
        // with an if expression before dispatching.
        let mut region = dispatch;
        let base_visited = visited.clone();
        let mut emitted_blocks = base_visited.clone();
        for (index, (_, target)) in cases.iter().enumerate() {
            let mut arm_body = Body::new();
            let mut arm_visited = base_visited.clone();
            let exit =
                self.emit_linear_region(*target, Some(join), &mut arm_visited, &mut arm_body)?;
            emitted_blocks.extend(arm_visited);
            if let RegionExit::Value(value) = exit {
                arm_body.push(Op::Leaf(Instruction::LocalGet(local(
                    &self.locals,
                    value,
                    span,
                )?)));
                arm_body.push(Op::Leaf(Instruction::LocalSet(local(
                    &self.locals,
                    join_value,
                    span,
                )?)));
            }
            arm_body.push(Op::Leaf(Instruction::Br(case_count - index as u32)));

            let mut wrapped = vec![Op::Block {
                body: region,
                result: None,
                span,
            }];
            wrapped.extend(arm_body);
            region = wrapped;
        }

        let mut default_body = Body::new();
        let mut default_visited = base_visited;
        let exit =
            self.emit_linear_region(default, Some(join), &mut default_visited, &mut default_body)?;
        emitted_blocks.extend(default_visited);
        if let RegionExit::Value(value) = exit {
            default_body.push(Op::Leaf(Instruction::LocalGet(local(
                &self.locals,
                value,
                span,
            )?)));
            default_body.push(Op::Leaf(Instruction::LocalSet(local(
                &self.locals,
                join_value,
                span,
            )?)));
        }
        default_body.push(Op::Leaf(Instruction::Br(0)));
        let mut with_default = vec![Op::Block {
            body: region,
            result: None,
            span,
        }];
        with_default.extend(default_body);
        body.push(Op::Block {
            body: with_default,
            result: None,
            span,
        });
        *visited = emitted_blocks;
        Ok(join)
    }
}

fn switch_index(
    value: ValueId,
    cases: &[(i32, crate::mir::BlockId)],
    locals: &HashMap<ValueId, u32>,
    span: psrs_span::TextRange,
) -> Result<Op, Vec<BackendError>> {
    let mut result = Body::new();
    if cases.is_empty() {
        result.push(Op::Leaf(Instruction::I32Const(0)));
    } else {
        for (index, (tag, _)) in cases.iter().enumerate().rev() {
            let else_body = if index + 1 == cases.len() {
                vec![Op::Leaf(Instruction::I32Const(cases.len() as i32))]
            } else {
                std::mem::take(&mut result)
            };
            result = vec![
                Op::Leaf(Instruction::LocalGet(local(locals, value, span)?)),
                Op::Leaf(Instruction::I32Const(*tag)),
                Op::Leaf(Instruction::I32Eq),
                Op::If {
                    then_body: vec![Op::Leaf(Instruction::I32Const(index as i32))],
                    else_body,
                    result: Some(ValType::I32),
                    span,
                },
            ];
        }
    }
    Ok(Op::Block {
        body: result,
        result: Some(ValType::I32),
        span,
    })
}

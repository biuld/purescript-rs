use super::cfg::{ControlFlowPlan, RegionPlan, UnitPlan, unit_span};
use super::{Structurer, local, wasm_error};
use crate::BackendError;
use crate::mir::{BlockId, Terminator};
use crate::types::ValueId;
use crate::wasm::{Body, Op};
use std::borrow::Cow;
use wasm_encoder::{Instruction, ValType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Label {
    Block(BlockId),
    Loop(BlockId),
}

pub(super) trait RegionOps {
    fn emit_control_flow(
        &self,
        plan: &ControlFlowPlan,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>>;

    fn emit_region(
        &self,
        region: &RegionPlan,
        enclosing: &[Label],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>>;

    fn emit_unit(
        &self,
        unit: &UnitPlan,
        labels: &[Label],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>>;
}

impl RegionOps for Structurer<'_> {
    fn emit_control_flow(
        &self,
        plan: &ControlFlowPlan,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        self.emit_region(&plan.root, &[], body)
    }

    fn emit_region(
        &self,
        region: &RegionPlan,
        enclosing: &[Label],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        let mut sequence = Body::new();
        for (index, unit) in region.units.iter().enumerate() {
            let span = unit_span(unit, &self.blocks, region.span);
            sequence = vec![Op::Block {
                body: sequence,
                result: None,
                span,
            }];

            let mut labels = enclosing.to_vec();
            if let Some(header) = region.header {
                labels.push(Label::Loop(header));
            }
            labels.extend(
                region.units[index + 1..]
                    .iter()
                    .rev()
                    .map(|target| Label::Block(target.entry())),
            );
            self.emit_unit(unit, &labels, &mut sequence)?;
        }
        body.extend(sequence);
        Ok(())
    }

    fn emit_unit(
        &self,
        unit: &UnitPlan,
        labels: &[Label],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        match unit {
            UnitPlan::Block(block_id) => {
                let block = self
                    .blocks
                    .get(block_id)
                    .ok_or_else(|| wasm_error(self.function.span, "MIR block does not exist"))?;
                self.emit_block_instructions(&block.instructions, body)?;
                self.emit_terminator(block, labels, body)
            }
            UnitPlan::Loop {
                header,
                body: loop_body,
            } => {
                let mut structured_body = Body::new();
                self.emit_region(loop_body, labels, &mut structured_body)?;
                body.push(Op::Loop {
                    body: structured_body,
                    result: None,
                    span: unit_span(unit, &self.blocks, self.function.span),
                });
                if loop_body.header != Some(*header) {
                    return Err(wasm_error(
                        self.function.span,
                        "MIR natural-loop plan has a mismatched header",
                    ));
                }
                Ok(())
            }
        }
    }
}

impl Structurer<'_> {
    fn emit_terminator(
        &self,
        block: &crate::mir::BasicBlock,
        labels: &[Label],
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
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
            }
            Terminator::Jump {
                target,
                arguments,
                span,
            } => {
                self.emit_jump_arguments(*target, arguments, *span, body)?;
                let depth = branch_depth(*target, labels, *span)?;
                body.push(Op::Leaf(Instruction::Br(depth)));
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
                span,
                ..
            } => {
                self.require_parameterless_target(*then_block, *span)?;
                self.require_parameterless_target(*else_block, *span)?;
                let then_depth = branch_depth(*then_block, labels, *span)?;
                let else_depth = branch_depth(*else_block, labels, *span)?;
                body.push(Op::Leaf(Instruction::LocalGet(local(
                    &self.locals,
                    *condition,
                    *span,
                )?)));
                body.push(Op::Leaf(Instruction::BrIf(then_depth)));
                body.push(Op::Leaf(Instruction::Br(else_depth)));
            }
            Terminator::Switch {
                value,
                cases,
                default,
                span,
            } => {
                self.require_parameterless_target(*default, *span)?;
                for (_, target) in cases {
                    self.require_parameterless_target(*target, *span)?;
                }
                body.push(switch_index(*value, cases, &self.locals, *span)?);
                let depths = cases
                    .iter()
                    .map(|(_, target)| branch_depth(*target, labels, *span))
                    .collect::<Result<Vec<_>, _>>()?;
                let default_depth = branch_depth(*default, labels, *span)?;
                body.push(Op::Leaf(Instruction::BrTable(
                    Cow::Owned(depths),
                    default_depth,
                )));
            }
        }
        Ok(())
    }

    fn emit_jump_arguments(
        &self,
        target: BlockId,
        arguments: &[ValueId],
        span: psrs_span::TextRange,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        let target_block = self
            .blocks
            .get(&target)
            .ok_or_else(|| wasm_error(span, "MIR jump target is missing"))?;
        if target_block.parameters.len() != arguments.len() {
            return Err(wasm_error(
                span,
                "MIR jump argument count differs from block parameters",
            ));
        }
        // Read all arguments before writing any target parameter. This keeps
        // loop-carried permutations correct when a jump swaps block values.
        for argument in arguments {
            body.push(Op::Leaf(Instruction::LocalGet(local(
                &self.locals,
                *argument,
                span,
            )?)));
        }
        for parameter in target_block.parameters.iter().rev() {
            body.push(Op::Leaf(Instruction::LocalSet(local(
                &self.locals,
                *parameter,
                span,
            )?)));
        }
        Ok(())
    }

    fn require_parameterless_target(
        &self,
        target: BlockId,
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let block = self
            .blocks
            .get(&target)
            .ok_or_else(|| wasm_error(span, "MIR branch target is missing"))?;
        if block.parameters.is_empty() {
            Ok(())
        } else {
            Err(wasm_error(
                span,
                "MIR branch target has block parameters but the terminator passes no arguments",
            ))
        }
    }
}

fn branch_depth(
    target: BlockId,
    labels: &[Label],
    span: psrs_span::TextRange,
) -> Result<u32, Vec<BackendError>> {
    let label = labels
        .iter()
        .rposition(|label| *label == Label::Loop(target))
        .or_else(|| {
            labels
                .iter()
                .rposition(|label| *label == Label::Block(target))
        })
        .ok_or_else(|| {
            wasm_error(
                span,
                "MIR branch target is not an enclosing Wasm block or loop label",
            )
        })?;
    Ok((labels.len() - label - 1) as u32)
}

fn switch_index(
    value: ValueId,
    cases: &[(i32, BlockId)],
    locals: &std::collections::HashMap<ValueId, u32>,
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

use super::*;
use crate::mir::UnaryOp;
use wasm_encoder::ValType;

impl Structurer<'_> {
    pub(super) fn emit_unary_primitive(
        &self,
        destination: ValueId,
        op: UnaryOp,
        value: ValueId,
        span: psrs_span::TextRange,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        let value_local = local(&self.locals, value, span)?;
        match op {
            UnaryOp::I32Neg => {
                body.push(Op::Leaf(Instruction::I32Const(0)));
                body.push(Op::Leaf(Instruction::LocalGet(value_local)));
                body.push(Op::Leaf(Instruction::I32Sub));
            }
            UnaryOp::I32Complement => {
                body.push(Op::Leaf(Instruction::LocalGet(value_local)));
                body.push(Op::Leaf(Instruction::I32Const(-1)));
                body.push(Op::Leaf(Instruction::I32Xor));
            }
            UnaryOp::F64Neg => {
                body.push(Op::Leaf(Instruction::LocalGet(value_local)));
                body.push(Op::Leaf(Instruction::F64Neg));
            }
            UnaryOp::BoolNot => {
                body.push(Op::Leaf(Instruction::LocalGet(value_local)));
                body.push(Op::Leaf(Instruction::I32Eqz));
            }
            UnaryOp::I32ToF64 => {
                body.push(Op::Leaf(Instruction::LocalGet(value_local)));
                body.push(Op::Leaf(Instruction::F64ConvertI32S));
            }
            UnaryOp::F64ToF32 => {
                body.push(Op::Leaf(Instruction::LocalGet(value_local)));
                body.push(Op::Leaf(Instruction::F32DemoteF64));
            }
            UnaryOp::F32ToF64 => {
                body.push(Op::Leaf(Instruction::LocalGet(value_local)));
                body.push(Op::Leaf(Instruction::F64PromoteF32));
            }
            UnaryOp::F64ToI32Sat => emit_saturating_f64_to_i32(value_local, span, body),
            UnaryOp::BoolToI32 | UnaryOp::I32Identity => {
                body.push(Op::Leaf(Instruction::LocalGet(value_local)));
            }
            UnaryOp::I32ToBool => {
                body.push(Op::Leaf(Instruction::LocalGet(value_local)));
                body.push(Op::Leaf(Instruction::I32Const(0)));
                body.push(Op::Leaf(Instruction::I32Ne));
            }
        }
        body.push(Op::Leaf(Instruction::LocalSet(local(
            &self.locals,
            destination,
            span,
        )?)));
        Ok(())
    }
}

fn emit_saturating_f64_to_i32(value_local: u32, span: psrs_span::TextRange, body: &mut Body) {
    let truncate = vec![
        Op::Leaf(Instruction::LocalGet(value_local)),
        Op::Leaf(Instruction::I32TruncF64S),
    ];
    let upper_bound = vec![
        Op::Leaf(Instruction::LocalGet(value_local)),
        Op::Leaf(Instruction::F64Const(2_147_483_648.0.into())),
        Op::Leaf(Instruction::F64Ge),
        Op::If {
            then_body: vec![Op::Leaf(Instruction::I32Const(i32::MAX))],
            else_body: truncate,
            result: Some(ValType::I32),
            span,
        },
    ];
    let lower_bound = vec![
        Op::Leaf(Instruction::LocalGet(value_local)),
        Op::Leaf(Instruction::F64Const((-2_147_483_648.0).into())),
        Op::Leaf(Instruction::F64Le),
        Op::If {
            then_body: vec![Op::Leaf(Instruction::I32Const(i32::MIN))],
            else_body: upper_bound,
            result: Some(ValType::I32),
            span,
        },
    ];
    body.extend([
        Op::Leaf(Instruction::LocalGet(value_local)),
        Op::Leaf(Instruction::LocalGet(value_local)),
        Op::Leaf(Instruction::F64Ne),
        Op::If {
            then_body: vec![Op::Leaf(Instruction::I32Const(0))],
            else_body: lower_bound,
            result: Some(ValType::I32),
            span,
        },
    ]);
}

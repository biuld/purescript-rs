use super::super::super::BlockId;
use super::super::super::instruction::Instruction;
use super::super::WitCallLowerer;
use crate::BackendError;
use crate::mir::NumericOp;
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;

/// Masks a source `Int` to a WIT narrow integer width. A signed narrow type is
/// then sign-extended so the canonical `i32` is an in-range signed value.
pub(super) fn narrow_integer<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    bits: u8,
    signed: bool,
    current: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let mask = (1i32 << bits) - 1;
    let mask_value = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Constant {
            destination: mask_value,
            value: mask,
            span,
        },
        span,
    )?;
    let masked = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Primitive {
            destination: masked,
            op: NumericOp::I32And,
            left: argument,
            right: mask_value,
            span,
        },
        span,
    )?;
    if !signed {
        return Ok(masked);
    }
    let shift = 32 - bits;
    let shift_value = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Constant {
            destination: shift_value,
            value: shift as i32,
            span,
        },
        span,
    )?;
    let shifted = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Primitive {
            destination: shifted,
            op: NumericOp::I32Shl,
            left: masked,
            right: shift_value,
            span,
        },
        span,
    )?;
    let sign_extended = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Primitive {
            destination: sign_extended,
            op: NumericOp::I32ShrS,
            left: shifted,
            right: shift_value,
            span,
        },
        span,
    )?;
    Ok(sign_extended)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder {
        next: u32,
        instructions: Vec<Instruction>,
        primitives: Vec<NumericOp>,
    }

    impl WitCallLowerer for Recorder {
        fn fresh_wit_value(&mut self, _ty: ValueType) -> ValueId {
            let id = ValueId(self.next);
            self.next += 1;
            id
        }

        fn append_wit_instruction(
            &mut self,
            _block: BlockId,
            instruction: Instruction,
            _span: TextRange,
        ) -> Result<(), Vec<BackendError>> {
            if let Instruction::Primitive { op, .. } = &instruction {
                self.primitives.push(*op);
            }
            self.instructions.push(instruction);
            Ok(())
        }

        fn wit_product_field(
            &mut self,
            _block: BlockId,
            _value: ValueId,
            _field: u32,
            _span: TextRange,
        ) -> Result<ValueId, Vec<BackendError>> {
            Ok(ValueId(0))
        }
    }

    #[test]
    fn unsigned_narrow_parameter_only_masks() {
        let mut recorder = Recorder::default();
        let _ = narrow_integer(
            &mut recorder,
            ValueId(0),
            8,
            false,
            BlockId(0),
            TextRange::new(0, 1),
        )
        .expect("masking an unsigned narrow integer");
        assert_eq!(recorder.primitives, vec![NumericOp::I32And]);
    }

    #[test]
    fn signed_narrow_parameter_masks_and_sign_extends() {
        let mut recorder = Recorder::default();
        let _ = narrow_integer(
            &mut recorder,
            ValueId(0),
            16,
            true,
            BlockId(0),
            TextRange::new(0, 1),
        )
        .expect("masking and sign-extending a signed narrow integer");
        assert_eq!(
            recorder.primitives,
            vec![NumericOp::I32And, NumericOp::I32Shl, NumericOp::I32ShrS]
        );
    }
}

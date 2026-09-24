use super::super::*;
use crate::abi::{SourceSignature, SourceType};
use std::collections::HashMap;
#[derive(Default)]
pub(super) struct RecordingLowerer {
    pub(super) next_value: u32,
    pub(super) instructions: Vec<Instruction>,
    pub(super) product_fields: Vec<u32>,
    pub(super) product_field_types: HashMap<u32, ValueType>,
}

impl WitCallLowerer for RecordingLowerer {
    fn fresh_wit_value(&mut self, _ty: ValueType) -> ValueId {
        let value = ValueId(self.next_value);
        self.next_value += 1;
        value
    }

    fn append_wit_instruction(
        &mut self,
        _block: BlockId,
        instruction: Instruction,
        _span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.instructions.push(instruction);
        Ok(())
    }

    fn wit_product_field(
        &mut self,
        _block: BlockId,
        value: ValueId,
        field: u32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let ty = self
            .product_field_types
            .get(&field)
            .copied()
            .unwrap_or(ValueType::I32);
        let destination = self.fresh_wit_value(ty);
        self.product_fields.push(field);
        self.instructions.push(Instruction::Copy {
            destination,
            value,
            span,
        });
        Ok(destination)
    }
}
pub(super) fn source_signature(parameters: Vec<SourceType>, result: SourceType) -> SourceSignature {
    SourceSignature {
        parameters,
        result,
        span: TextRange::new(0, 1),
    }
}

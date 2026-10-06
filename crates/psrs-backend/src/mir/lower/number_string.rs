//! Checked raw runtime formatting followed by canonical UTF-8 recovery.
use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn lower_number_to_string(
        &mut self,
        block: BlockId,
        destination: ValueId,
        value: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let implementation =
            crate::target_runtime::implementation(psrs_hir::Intrinsic::NumberToString)
                .expect("NumberToString has a registered target implementation");
        let zero = self.constant(block, 0, span)?;
        let align = self.constant(block, 1, span)?;
        let capacity = self.constant(block, implementation.abi.output_capacity as i32, span)?;
        let buffer = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::Call {
                destination: buffer,
                function: crate::abi::REALLOC_SYMBOL,
                arguments: vec![zero, zero, align, capacity],
                span,
            },
            span,
        )?;
        let length = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::Call {
                destination: length,
                function: implementation.symbol,
                arguments: vec![value, buffer, capacity],
                span,
            },
            span,
        )?;
        self.append_instruction(
            block,
            Instruction::Call {
                destination,
                function: crate::abi::BYTES_TO_STRING_SYMBOL,
                arguments: vec![buffer, length],
                span,
            },
            span,
        )?;
        let discarded = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::Call {
                destination: discarded,
                function: crate::abi::REALLOC_SYMBOL,
                arguments: vec![buffer, capacity, align, zero],
                span,
            },
            span,
        )
    }
}

//! Linear-memory lowering for array cloning.

use super::LinearFunctionLowerer;
use crate::BackendError;
use crate::cc::ReprId;
use crate::mir::{BlockId, Instruction};
use crate::types::ValueId;
use psrs_span::TextRange;

impl LinearFunctionLowerer<'_> {
    pub(super) fn lower_array_clone(
        &mut self,
        current: BlockId,
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let (_, stride, element_offset, alignment) = self
            .layout
            .array(representation)
            .map_err(|error| super::layout_error(span, error))?;
        let length = self.fresh(crate::types::ValueType::I32);
        self.append(
            current,
            Instruction::LinearLoad {
                memory: crate::types::MemoryId(0),
                alignment: 4,
                destination: length,
                address: value,
                offset: 0,
                object_bytes: element_offset,
                ty: crate::types::ValueType::I32,
                span,
            },
            span,
        )?;
        if stride == 0 {
            return Err(super::unsupported(
                span,
                "linear-memory array element stride must be nonzero",
            ));
        }
        let maximum_payload = (i32::MAX as u32)
            .checked_sub(element_offset)
            .ok_or_else(|| {
                super::unsupported(
                    span,
                    "linear-memory array header exceeds the i32 size limit",
                )
            })?;
        let maximum_length = i32::try_from(maximum_payload / stride)
            .map_err(|_| super::unsupported(span, "linear-memory array length exceeds i32"))?;
        let zero = self.fresh(crate::types::ValueType::I32);
        self.append(
            current,
            Instruction::Constant {
                destination: zero,
                value: 0,
                span,
            },
            span,
        )?;
        let negative_length = self.fresh(crate::types::ValueType::Boolean);
        self.append(
            current,
            Instruction::Primitive {
                destination: negative_length,
                op: super::super::NumericOp::I32LtS,
                left: length,
                right: zero,
                span,
            },
            span,
        )?;
        self.append(
            current,
            Instruction::TrapIf {
                condition: negative_length,
                span,
            },
            span,
        )?;
        let maximum_length_value = self.fresh(crate::types::ValueType::I32);
        self.append(
            current,
            Instruction::Constant {
                destination: maximum_length_value,
                value: maximum_length,
                span,
            },
            span,
        )?;
        let excessive_length = self.fresh(crate::types::ValueType::Boolean);
        self.append(
            current,
            Instruction::Primitive {
                destination: excessive_length,
                op: super::super::NumericOp::I32GtS,
                left: length,
                right: maximum_length_value,
                span,
            },
            span,
        )?;
        self.append(
            current,
            Instruction::TrapIf {
                condition: excessive_length,
                span,
            },
            span,
        )?;
        let stride_value = self.fresh(crate::types::ValueType::I32);
        self.append(
            current,
            Instruction::Constant {
                destination: stride_value,
                value: stride as i32,
                span,
            },
            span,
        )?;
        let payload_bytes = self.fresh(crate::types::ValueType::I32);
        self.append(
            current,
            Instruction::Primitive {
                destination: payload_bytes,
                op: super::super::NumericOp::I32Mul,
                left: length,
                right: stride_value,
                span,
            },
            span,
        )?;
        let header_bytes = self.fresh(crate::types::ValueType::I32);
        self.append(
            current,
            Instruction::Constant {
                destination: header_bytes,
                value: element_offset as i32,
                span,
            },
            span,
        )?;
        let bytes = self.fresh(crate::types::ValueType::I32);
        self.append(
            current,
            Instruction::Primitive {
                destination: bytes,
                op: super::super::NumericOp::I32Add,
                left: header_bytes,
                right: payload_bytes,
                span,
            },
            span,
        )?;
        self.append(
            current,
            Instruction::LinearAllocDynamic {
                destination,
                bytes,
                alignment,
                span,
            },
            span,
        )?;
        self.append(
            current,
            Instruction::LinearStore {
                memory: crate::types::MemoryId(0),
                alignment: 4,
                address: destination,
                value: length,
                offset: 0,
                object_bytes: element_offset,
                ty: crate::types::ValueType::I32,
                span,
            },
            span,
        )?;
        self.append(
            current,
            Instruction::LinearMemoryCopy {
                destination,
                source: value,
                bytes: payload_bytes,
                destination_offset: element_offset,
                source_offset: element_offset,
                span,
            },
            span,
        )
    }
}

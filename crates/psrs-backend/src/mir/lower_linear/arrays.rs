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
        let (_, stride) = self
            .layout
            .array(representation)
            .map_err(|error| super::layout_error(span, error))?;
        let length = self.fresh(crate::types::ValueType::I32);
        self.append(
            current,
            Instruction::LinearLoad {
                destination: length,
                address: value,
                offset: 0,
                ty: crate::types::ValueType::I32,
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
                op: psrs_core::Primitive::Mul,
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
                value: 4,
                span,
            },
            span,
        )?;
        let bytes = self.fresh(crate::types::ValueType::I32);
        self.append(
            current,
            Instruction::Primitive {
                destination: bytes,
                op: psrs_core::Primitive::Add,
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
                span,
            },
            span,
        )?;
        self.append(
            current,
            Instruction::LinearStore {
                address: destination,
                value: length,
                offset: 0,
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
                destination_offset: 4,
                source_offset: 4,
                span,
            },
            span,
        )
    }
}

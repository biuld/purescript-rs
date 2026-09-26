//! Array element projection lowering.

use super::{FunctionLowerer, layout_error};
use crate::BackendError;
use crate::cc::ReprId;
use crate::mir::instruction::Instruction;
use crate::types::{RefType, ValueId, ValueType};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    /// Lowers `array.get`. A non-null reference element needs a nullable
    /// temporary plus a `ref.cast`, because the array element storage is
    /// nullable while the destination is not.
    pub(super) fn lower_array_get(
        &mut self,
        current: super::BlockId,
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
        index: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let type_index = self
            .layout
            .repr_index(representation)
            .map_err(|error| layout_error(span, error))?;
        let destination_type = self
            .values
            .iter()
            .find(|candidate| candidate.id == destination)
            .map(|candidate| candidate.ty)
            .ok_or_else(|| {
                vec![BackendError::invalid_ir(
                    "P9 MIR lowering",
                    span,
                    "array.get destination has no value declaration",
                )]
            })?;
        if let ValueType::Ref(reference) = destination_type
            && !reference.nullable
        {
            let temporary = self.fresh(ValueType::Ref(RefType {
                nullable: true,
                heap: reference.heap,
            }));
            self.append_instruction(
                current,
                Instruction::ArrayGet {
                    destination: temporary,
                    type_index,
                    value,
                    index,
                    span,
                },
                span,
            )?;
            self.append_instruction(
                current,
                Instruction::RefCast {
                    destination,
                    value: temporary,
                    reference,
                    span,
                },
                span,
            )?;
        } else {
            self.append_instruction(
                current,
                Instruction::ArrayGet {
                    destination,
                    type_index,
                    value,
                    index,
                    span,
                },
                span,
            )?;
        }
        Ok(())
    }
}

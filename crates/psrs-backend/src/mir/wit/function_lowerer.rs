use super::WitCallLowerer;
use crate::BackendError;
use crate::mir::lower::FunctionLowerer;
use crate::mir::{BlockId, Instruction};
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;

impl WitCallLowerer for FunctionLowerer<'_> {
    fn fresh_wit_value(&mut self, ty: ValueType) -> ValueId {
        self.fresh(ty)
    }

    fn append_wit_instruction(
        &mut self,
        block: BlockId,
        instruction: Instruction,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.append_instruction(block, instruction, span)
    }

    fn wit_product_field(
        &mut self,
        block: BlockId,
        value: ValueId,
        field: u32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        self.wit_product_field(block, value, field, span)
    }

    fn wit_product(
        &self,
        repr: crate::cc::ReprId,
    ) -> Option<(Vec<crate::cc::ValueShape>, Vec<String>)> {
        self.resolved_product(repr)
    }

    fn wit_array_element(&self, repr: crate::cc::ReprId) -> Option<crate::cc::ValueShape> {
        self.resolved_array_element(repr)
    }

    fn wit_repr_index(&self, repr: crate::cc::ReprId) -> Option<crate::types::DefinedTypeId> {
        self.resolved_repr_index(repr)
    }

    fn note_owned(&mut self, value: ValueId, drop_symbol: psrs_hir::SymbolId, span: TextRange) {
        self.note_owned_handle(value, drop_symbol, span);
    }

    fn transfer_owned(&mut self, value: ValueId) {
        self.transfer_owned_handle(value);
    }

    fn wit_array_type(
        &self,
        value: ValueId,
        span: TextRange,
    ) -> Result<crate::types::DefinedTypeId, Vec<BackendError>> {
        match self.value_type(value) {
            Some(ValueType::Ref(reference)) => match reference.heap {
                crate::types::HeapType::Index(index) => Ok(index),
                _ => Err(vec![BackendError::new(
                    "P9 MIR lowering",
                    span,
                    "canonical list value is not a concrete GC array",
                )]),
            },
            _ => Err(vec![BackendError::new(
                "P9 MIR lowering",
                span,
                "canonical list value is not a GC array",
            )]),
        }
    }
}

use super::WitCallLowerer;
use crate::BackendError;
use crate::cc::{Assignment, AssignmentKind, ReprId};
use crate::mir::lower::FunctionLowerer;
use crate::mir::{BlockId, Instruction, Terminator};
use crate::types::{DefinedTypeId, ValueId, ValueType};
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

    fn wit_new_block(&mut self, parameters: Vec<ValueId>) -> BlockId {
        self.new_block(parameters)
    }

    fn wit_jump(
        &mut self,
        block: BlockId,
        target: BlockId,
        arguments: Vec<ValueId>,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.set_terminator(
            block,
            Terminator::Jump {
                target,
                arguments,
                span,
            },
            span,
        )
    }

    fn wit_switch(
        &mut self,
        block: BlockId,
        value: ValueId,
        cases: Vec<(i32, BlockId)>,
        default: BlockId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.set_terminator(
            block,
            Terminator::Switch {
                value,
                cases,
                default,
                span,
            },
            span,
        )
    }

    fn wit_variant_tag(
        &mut self,
        block: BlockId,
        destination: ValueId,
        representation: ReprId,
        value: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.lower_variant(
            &Assignment {
                destination,
                kind: AssignmentKind::VariantTag {
                    destination,
                    representation,
                    value,
                },
                span,
            },
            block,
        )
    }

    fn wit_variant_get(
        &mut self,
        block: BlockId,
        destination: ValueId,
        representation: ReprId,
        case: u32,
        field: u32,
        value: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.lower_variant(
            &Assignment {
                destination,
                kind: AssignmentKind::VariantGet {
                    destination,
                    representation,
                    case,
                    field,
                    value,
                },
                span,
            },
            block,
        )
    }

    fn wit_variant_new(
        &mut self,
        block: BlockId,
        destination: ValueId,
        representation: ReprId,
        case: u32,
        fields: Vec<ValueId>,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.lower_variant(
            &Assignment {
                destination,
                kind: AssignmentKind::VariantNew {
                    destination,
                    representation,
                    case,
                    fields,
                },
                span,
            },
            block,
        )
    }

    fn wit_boxed_integer(&self) -> Option<DefinedTypeId> {
        self.wit_boxed_integer()
    }

    fn wit_boxed_number(&self) -> Option<DefinedTypeId> {
        self.wit_boxed_number()
    }

    fn wit_string_index(&self) -> Option<DefinedTypeId> {
        self.wit_string_index()
    }

    fn wit_case_field_type(
        &self,
        representation: ReprId,
        case: u32,
        field: u32,
    ) -> Option<ValueType> {
        self.wit_case_field_type(representation, case, field)
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

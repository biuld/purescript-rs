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

    fn wit_guest_layout(&self, shape: crate::cc::ValueShape) -> Option<crate::cc::GuestLayout> {
        self.resolved_guest_layout(shape)
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

    fn wit_array_get(
        &mut self,
        block: BlockId,
        array: ValueId,
        array_type: crate::types::DefinedTypeId,
        element: crate::cc::ValueShape,
        index: u32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let index_value = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::Constant {
                destination: index_value,
                value: index as i32,
                span,
            },
            span,
        )?;
        let element_type = self.resolved_value_type(&element).ok_or_else(|| {
            vec![BackendError::new(
                "P9 MIR lowering",
                span,
                "fixed-length list element has no value type",
            )]
        })?;
        if let ValueType::Ref(reference) = element_type
            && !reference.nullable
        {
            let temporary = self.fresh(ValueType::Ref(crate::types::RefType {
                nullable: true,
                heap: reference.heap,
            }));
            self.append_instruction(
                block,
                Instruction::ArrayGet {
                    destination: temporary,
                    type_index: array_type,
                    value: array,
                    index: index_value,
                    span,
                },
                span,
            )?;
            let destination = self.fresh(element_type);
            self.append_instruction(
                block,
                Instruction::RefCast {
                    destination,
                    value: temporary,
                    reference,
                    span,
                },
                span,
            )?;
            Ok(destination)
        } else {
            let destination = self.fresh(element_type);
            self.append_instruction(
                block,
                Instruction::ArrayGet {
                    destination,
                    type_index: array_type,
                    value: array,
                    index: index_value,
                    span,
                },
                span,
            )?;
            Ok(destination)
        }
    }
}

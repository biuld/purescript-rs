use super::super::*;
use crate::cc::{RefShape, Reference, ReprId, Signature, ValueShape};
use crate::mir::Terminator;
use crate::types::{DefinedTypeId, ValueType};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct RecordingLowerer {
    pub(super) next_value: u32,
    pub(super) next_block: u32,
    pub(super) instructions: Vec<Instruction>,
    pub(super) blocks: Vec<BlockId>,
    pub(super) terminators: Vec<(BlockId, Terminator)>,
    pub(super) product_fields: Vec<u32>,
    pub(super) product_field_types: HashMap<u32, ValueType>,
    pub(super) array_types: HashMap<ValueId, DefinedTypeId>,
    /// Record products by representation handle, with canonical labels. The
    /// adapter projects WIT fields by name through these.
    pub(super) products: HashMap<ReprId, (Vec<ValueShape>, Vec<String>)>,
    pub(super) boxed_integer: Option<DefinedTypeId>,
    pub(super) string_index: Option<DefinedTypeId>,
    pub(super) case_field_types: HashMap<(ReprId, u32, u32), ValueType>,
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

    fn wit_new_block(&mut self, _parameters: Vec<ValueId>) -> BlockId {
        let block = BlockId(self.next_block);
        self.next_block += 1;
        self.blocks.push(block);
        block
    }

    fn wit_jump(
        &mut self,
        block: BlockId,
        target: BlockId,
        arguments: Vec<ValueId>,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.terminators.push((
            block,
            Terminator::Jump {
                target,
                arguments,
                span,
            },
        ));
        Ok(())
    }

    fn wit_switch(
        &mut self,
        block: BlockId,
        value: ValueId,
        cases: Vec<(i32, BlockId)>,
        default: BlockId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.terminators.push((
            block,
            Terminator::Switch {
                value,
                cases,
                default,
                span,
            },
        ));
        Ok(())
    }

    fn wit_variant_tag(
        &mut self,
        _block: BlockId,
        destination: ValueId,
        _representation: ReprId,
        value: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.instructions.push(Instruction::Copy {
            destination,
            value,
            span,
        });
        Ok(())
    }

    fn wit_variant_get(
        &mut self,
        _block: BlockId,
        destination: ValueId,
        _representation: ReprId,
        _case: u32,
        _field: u32,
        value: ValueId,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.instructions.push(Instruction::Copy {
            destination,
            value,
            span,
        });
        Ok(())
    }

    fn wit_variant_new(
        &mut self,
        _block: BlockId,
        destination: ValueId,
        _representation: ReprId,
        _case: u32,
        _fields: Vec<ValueId>,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.instructions.push(Instruction::RefNull {
            destination,
            heap: crate::types::HeapType::Struct,
            span,
        });
        Ok(())
    }

    fn wit_boxed_integer(&self) -> Option<DefinedTypeId> {
        self.boxed_integer
    }

    fn wit_string_index(&self) -> Option<DefinedTypeId> {
        self.string_index
    }

    fn wit_case_field_type(
        &self,
        representation: ReprId,
        case: u32,
        field: u32,
    ) -> Option<ValueType> {
        self.case_field_types
            .get(&(representation, case, field))
            .copied()
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

    fn wit_array_type(
        &self,
        value: ValueId,
        span: TextRange,
    ) -> Result<DefinedTypeId, Vec<BackendError>> {
        self.array_types.get(&value).copied().ok_or_else(|| {
            vec![BackendError::new(
                "P9 MIR lowering",
                span,
                "canonical list lowering has no GC array type",
            )]
        })
    }

    fn wit_product(&self, repr: ReprId) -> Option<(Vec<ValueShape>, Vec<String>)> {
        self.products.get(&repr).cloned()
    }
}

/// A CC abstract signature from explicit parameter shapes. The WIT adapter only
/// reads the parameters; scalar-result tests set an arbitrary result shape.
pub(super) fn signature(parameters: Vec<ValueShape>) -> Signature {
    Signature {
        parameters,
        result: ValueShape::Integer,
    }
}

/// A record reference parameter shape for the given representation handle.
pub(super) fn reference(repr: u32) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(ReprId(repr)),
    })
}

/// Registers a record product for a `RecordingLowerer` and returns the shape.
pub(super) fn record(
    lowerer: &mut RecordingLowerer,
    repr: u32,
    labels: &[&str],
    fields: Vec<ValueShape>,
) -> ValueShape {
    lowerer.products.insert(
        ReprId(repr),
        (
            fields,
            labels.iter().map(|label| (*label).to_string()).collect(),
        ),
    );
    reference(repr)
}

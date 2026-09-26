use super::super::*;
use crate::cc::{RefShape, Reference, ReprId, Signature, ValueShape};
use crate::types::DefinedTypeId;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct RecordingLowerer {
    pub(super) next_value: u32,
    pub(super) instructions: Vec<Instruction>,
    pub(super) product_fields: Vec<u32>,
    pub(super) product_field_types: HashMap<u32, ValueType>,
    pub(super) array_types: HashMap<ValueId, DefinedTypeId>,
    /// Record products by representation handle, with canonical labels. The
    /// adapter projects WIT fields by name through these.
    pub(super) products: HashMap<ReprId, (Vec<ValueShape>, Vec<String>)>,
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

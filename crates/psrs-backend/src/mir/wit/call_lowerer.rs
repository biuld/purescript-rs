//! The lowering interface the canonical WIT adapter uses for a function.

use super::BlockId;
use crate::BackendError;
use crate::cc::{GuestLayout, ReprId, ValueShape};
use crate::mir::instruction::Instruction;
use crate::types::{DefinedTypeId, ValueId, ValueType};
use psrs_span::TextRange;

pub(crate) trait WitCallLowerer {
    fn fresh_wit_value(&mut self, ty: ValueType) -> ValueId;
    fn append_wit_instruction(
        &mut self,
        block: BlockId,
        instruction: Instruction,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>>;

    /// Allocates an empty basic block with the given parameters.
    fn wit_new_block(&mut self, _parameters: Vec<ValueId>) -> BlockId {
        BlockId(0)
    }

    /// Terminates `block` with an unconditional jump to `target`.
    fn wit_jump(
        &mut self,
        _block: BlockId,
        _target: BlockId,
        _arguments: Vec<ValueId>,
        _span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        Ok(())
    }

    /// Terminates `block` with a closed integer-tag dispatch.
    fn wit_switch(
        &mut self,
        _block: BlockId,
        _value: ValueId,
        _cases: Vec<(i32, BlockId)>,
        _default: BlockId,
        _span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        Ok(())
    }

    /// Reads the tag of a source variant value.
    fn wit_variant_tag(
        &mut self,
        _block: BlockId,
        _destination: ValueId,
        _representation: ReprId,
        _value: ValueId,
        _span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        Ok(())
    }

    /// Extracts one field of a source variant case.
    #[allow(clippy::too_many_arguments)]
    fn wit_variant_get(
        &mut self,
        _block: BlockId,
        _destination: ValueId,
        _representation: ReprId,
        _case: u32,
        _field: u32,
        _value: ValueId,
        _span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        Ok(())
    }

    /// Builds a source variant case value.
    #[allow(clippy::too_many_arguments)]
    fn wit_variant_new(
        &mut self,
        _block: BlockId,
        _destination: ValueId,
        _representation: ReprId,
        _case: u32,
        _fields: Vec<ValueId>,
        _span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        Ok(())
    }

    /// The target representation of the boxed integer, when reachable.
    fn wit_boxed_integer(&self) -> Option<DefinedTypeId> {
        None
    }

    /// The target representation of the boxed number, when reachable.
    fn wit_boxed_number(&self) -> Option<DefinedTypeId> {
        None
    }

    /// The concrete GC string type, when reachable.
    fn wit_string_index(&self) -> Option<DefinedTypeId> {
        None
    }

    /// The concrete MIR type of a source variant case field.
    fn wit_case_field_type(
        &self,
        _representation: ReprId,
        _case: u32,
        _field: u32,
    ) -> Option<ValueType> {
        None
    }
    fn wit_product_field(
        &mut self,
        block: BlockId,
        value: ValueId,
        field: u32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>>;

    /// Resolves a value shape to its recursive guest layout through the
    /// representation table. The canonical lowering walks this in lockstep with
    /// the canonical type. `None` when the lowerer has no representation table
    /// or the shape names an unknown representation.
    fn wit_guest_layout(&self, _shape: ValueShape) -> Option<GuestLayout> {
        None
    }

    /// Converts an aggregate into or out of its bare-slot storage protocol.
    fn wit_payload_conversion(
        &mut self,
        _block: BlockId,
        _value: ValueId,
        _shape: ValueShape,
        _entering: bool,
        span: TextRange,
    ) -> Result<(BlockId, ValueId), Vec<BackendError>> {
        Err(vec![BackendError::new(
            "P9 MIR lowering",
            span,
            "canonical aggregate payload has no storage conversion",
        )])
    }

    /// The concrete GC type of a representation handle.
    fn wit_repr_index(&self, _repr: crate::cc::ReprId) -> Option<crate::types::DefinedTypeId> {
        None
    }

    /// The GC array type of a source array value, when this lowerer has layouts.
    fn wit_array_type(
        &self,
        _value: ValueId,
        span: TextRange,
    ) -> Result<DefinedTypeId, Vec<BackendError>> {
        Err(vec![BackendError::new(
            "P9 MIR lowering",
            span,
            "canonical list lowering has no GC array type",
        )])
    }

    /// Projects the element at the constant `index` of a source GC array. Used
    /// to lower a fixed-length list, whose canonical form is inline.
    fn wit_array_get(
        &mut self,
        _block: BlockId,
        _array: ValueId,
        _array_type: DefinedTypeId,
        _element: ValueShape,
        _index: u32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        Err(vec![BackendError::new(
            "P9 MIR lowering",
            span,
            "canonical fixed-length list lowering has no array projection",
        )])
    }
}

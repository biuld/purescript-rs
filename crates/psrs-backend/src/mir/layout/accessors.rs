//! `PlannedLayout` accessors used by the MIR lowerer and verifier.

use super::{LayoutError, PlannedLayout, value_type};
use crate::cc::{
    RefShape as CcRefShape, Reference as CcReference, ReprId, SignatureId,
    ValueShape as CcValueShape,
};
use crate::types::{DefinedTypeId, HeapType, RefType, ValueType};

impl PlannedLayout {
    pub(in crate::mir) fn repr_index(&self, id: ReprId) -> Result<DefinedTypeId, LayoutError> {
        self.repr_indices
            .get(&id)
            .copied()
            .ok_or(LayoutError::UnknownRepresentation)
    }
    pub(in crate::mir) fn product_field(
        &self,
        id: DefinedTypeId,
        field: u32,
    ) -> Result<CcValueShape, LayoutError> {
        self.product_fields
            .get(&id)
            .and_then(|fields| fields.get(field as usize))
            .copied()
            .ok_or(LayoutError::UnknownField)
    }
    pub(in crate::mir) fn array_element(&self, id: ReprId) -> Result<CcValueShape, LayoutError> {
        self.array_elements
            .get(&id)
            .copied()
            .ok_or(LayoutError::UnknownRepresentation)
    }
    pub(in crate::mir) fn variant_index(
        &self,
        id: ReprId,
        case: u32,
    ) -> Result<DefinedTypeId, LayoutError> {
        self.variant_indices
            .get(&(id, case))
            .copied()
            .ok_or(LayoutError::UnknownField)
    }
    pub(in crate::mir) fn signature_index(
        &self,
        id: SignatureId,
    ) -> Result<DefinedTypeId, LayoutError> {
        self.signature_indices
            .get(&id)
            .copied()
            .ok_or(LayoutError::UnknownSignature)
    }

    pub(in crate::mir) fn closure_layout(
        &self,
    ) -> Result<(DefinedTypeId, DefinedTypeId), LayoutError> {
        self.closure_index
            .zip(self.capture_array_index)
            .ok_or(LayoutError::UnknownClosureLayout)
    }

    pub(in crate::mir) fn boxed_number_index(&self) -> Option<DefinedTypeId> {
        self.boxed_number_index
    }

    pub(in crate::mir) fn boxed_integer_index(&self) -> Option<DefinedTypeId> {
        self.boxed_integer_index
    }

    pub(in crate::mir) fn value_type(
        &self,
        value: &CcValueShape,
    ) -> Result<ValueType, LayoutError> {
        value_type(
            value,
            &self.repr_indices,
            self.closure_index,
            self.string_index,
        )
    }

    /// The GC string type index, present exactly when a `String` is reachable.
    pub(in crate::mir) fn string_index(&self) -> Option<DefinedTypeId> {
        self.string_index
    }

    pub(in crate::mir) fn reference(
        &self,
        reference: &CcReference,
    ) -> Result<RefType, LayoutError> {
        Ok(RefType {
            nullable: reference.nullable,
            heap: match reference.heap {
                CcRefShape::Repr(id) => HeapType::Index(self.repr_index(id)?),
                CcRefShape::Aggregate => HeapType::Struct,
                CcRefShape::Closure(_) => HeapType::Index(
                    self.closure_index
                        .ok_or(LayoutError::UnknownRepresentation)?,
                ),
                CcRefShape::Erased => HeapType::Eq,
            },
        })
    }
}

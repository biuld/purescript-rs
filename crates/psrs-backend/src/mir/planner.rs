//! P9 representation-planner contract and the linear-memory planning slice.
//!
//! The GC planner remains the emitter used by the current Wasm path.  This
//! module makes the ownership boundary explicit and proves that the same CC
//! requirements can be consumed by a second, non-GC planner.

use super::layout::{LayoutError, PlannedLayout};
use super::reachable::ReachableHandles;
use crate::TargetCapabilities;
use crate::cc::{Module as CcModule, ReprId, Representation, SignatureId, ValueShape};
use crate::types::{CompositeType, DefinedType, DefinedTypeId, ValueType};
use std::collections::HashMap;

/// A P9 planner consumes target-neutral CC requirements and produces a
/// target-specific layout description.  CC never depends on this trait or on
/// either concrete planner.
pub(super) trait RepresentationPlanner {
    type Layout;

    fn plan_module(&self, module: &CcModule) -> Result<Self::Layout, LayoutError>;
}

/// The current Wasm GC implementation of the planner contract.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct GcPlanner {
    pub(super) target: TargetCapabilities,
}

impl RepresentationPlanner for GcPlanner {
    type Layout = PlannedLayout;

    fn plan_module(&self, module: &CcModule) -> Result<Self::Layout, LayoutError> {
        PlannedLayout::plan_module(module, self.target)
    }
}

/// A compact linear-memory layout description.  It is intentionally separate
/// from MIR's GC type table: references become four-byte handles, products and
/// boxes become byte payloads, arrays use element strides, and closures use
/// table slots plus environment handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinearMemoryLayout {
    pub(super) representations: HashMap<ReprId, LinearRepresentation>,
    pub(super) signatures: HashMap<SignatureId, LinearSignature>,
    /// Function types used by the linear closure table. These are MIR-owned
    /// types, just like the GC planner's struct and array types.
    pub(super) types: Vec<crate::types::RecGroup>,
    pub(super) next_offset: u32,
}

impl LinearMemoryLayout {
    pub(super) fn representation(&self, id: ReprId) -> Result<&LinearRepresentation, LayoutError> {
        self.representations
            .get(&id)
            .ok_or(LayoutError::UnknownRepresentation)
    }

    pub(super) fn value_type(value: ValueShape) -> ValueType {
        match value {
            ValueShape::Integer => ValueType::I32,
            ValueShape::Boolean => ValueType::Boolean,
            ValueShape::Number => ValueType::F64,
            ValueShape::Reference(_) => ValueType::I32,
        }
    }

    pub(super) fn value_bytes(value: ValueShape) -> u32 {
        value_size(&value)
    }

    pub(super) fn value_alignment(value: ValueShape) -> u32 {
        value_alignment(&value)
    }

    pub(super) fn representation_size(&self, id: ReprId) -> Result<u32, LayoutError> {
        match self.representation(id)? {
            LinearRepresentation::Box {
                size, alignment, ..
            }
            | LinearRepresentation::Product {
                size, alignment, ..
            }
            | LinearRepresentation::Variant {
                size, alignment, ..
            } => Ok((*size).max(*alignment)),
            LinearRepresentation::Array { .. } => Err(LayoutError::UnsupportedLinearOperation),
        }
    }

    pub(super) fn representation_alignment(&self, id: ReprId) -> Result<u32, LayoutError> {
        let alignment = match self.representation(id)? {
            LinearRepresentation::Box { alignment, .. }
            | LinearRepresentation::Product { alignment, .. }
            | LinearRepresentation::Variant { alignment, .. }
            | LinearRepresentation::Array { alignment, .. } => *alignment,
        };
        Ok(alignment)
    }

    pub(super) fn field(&self, id: ReprId, field: u32) -> Result<(u32, ValueShape), LayoutError> {
        match self.representation(id)? {
            LinearRepresentation::Box { value, .. } => {
                if field == 0 {
                    Ok((0, *value))
                } else {
                    Err(LayoutError::UnknownField)
                }
            }
            LinearRepresentation::Product { fields, .. } => fields
                .get(field as usize)
                .map(|field| (field.offset, field.value))
                .ok_or(LayoutError::UnknownField),
            LinearRepresentation::Variant { .. } | LinearRepresentation::Array { .. } => {
                Err(LayoutError::UnknownField)
            }
        }
    }

    pub(super) fn array(&self, id: ReprId) -> Result<(ValueShape, u32, u32, u32), LayoutError> {
        match self.representation(id)? {
            LinearRepresentation::Array {
                element,
                stride,
                element_offset,
                alignment,
            } => Ok((*element, *stride, *element_offset, *alignment)),
            _ => Err(LayoutError::UnsupportedLinearOperation),
        }
    }

    pub(super) fn variant_fields(
        &self,
        id: ReprId,
        tag: u32,
    ) -> Result<&[LinearField], LayoutError> {
        match self.representation(id)? {
            LinearRepresentation::Variant { cases, .. } => cases
                .iter()
                .find(|(case_tag, _)| *case_tag == tag)
                .map(|(_, fields)| fields.as_slice())
                .ok_or(LayoutError::UnknownField),
            _ => Err(LayoutError::UnknownRepresentation),
        }
    }

    pub(super) fn signature_index(&self, id: SignatureId) -> Result<DefinedTypeId, LayoutError> {
        self.signatures
            .get(&id)
            .map(|signature| signature.type_index)
            .ok_or(LayoutError::UnknownSignature)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinearRepresentation {
    Box {
        value: ValueShape,
        size: u32,
        alignment: u32,
    },
    Product {
        fields: Vec<LinearField>,
        size: u32,
        alignment: u32,
    },
    Variant {
        tag_size: u32,
        cases: Vec<(u32, Vec<LinearField>)>,
        size: u32,
        alignment: u32,
    },
    Array {
        element: ValueShape,
        stride: u32,
        element_offset: u32,
        alignment: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinearField {
    pub(super) offset: u32,
    pub(super) value: ValueShape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinearSignature {
    pub(super) type_index: DefinedTypeId,
    pub(super) table_slot: u32,
    pub(super) parameters: Vec<ValueShape>,
    pub(super) result: ValueShape,
}

/// Linear-memory planner used as the non-GC M5 strategy. It consumes the same
/// reachable CC handles as the GC planner and exposes byte layouts to P9's
/// allocator/load/store instruction selection.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LinearMemoryPlanner;

impl RepresentationPlanner for LinearMemoryPlanner {
    type Layout = LinearMemoryLayout;

    fn plan_module(&self, module: &CcModule) -> Result<Self::Layout, LayoutError> {
        let reachable = ReachableHandles::from_module_without_gc_boxes(module)?;
        let mut representations = HashMap::new();
        let mut offset = 0u32;
        for id in reachable.representations {
            let representation = module
                .representations
                .representation(id)
                .ok_or(LayoutError::UnknownRepresentation)?;
            let (planned, size) = plan_representation(representation, &module.representations)?;
            let alignment = match &planned {
                LinearRepresentation::Box { alignment, .. }
                | LinearRepresentation::Product { alignment, .. }
                | LinearRepresentation::Variant { alignment, .. }
                | LinearRepresentation::Array { alignment, .. } => *alignment,
            };
            offset = align(offset, alignment);
            representations.insert(id, planned);
            offset = offset.saturating_add(size);
        }

        let mut signatures = HashMap::new();
        let mut signature_types = Vec::new();
        for (slot, id) in reachable.signatures.into_iter().enumerate() {
            let signature = module
                .representations
                .signature(id)
                .ok_or(LayoutError::UnknownSignature)?;
            signatures.insert(
                id,
                LinearSignature {
                    type_index: DefinedTypeId(slot as u32),
                    table_slot: slot as u32,
                    parameters: signature.parameters.clone(),
                    result: signature.result,
                },
            );
            let mut parameters = vec![ValueType::I32];
            parameters.extend(
                signature
                    .parameters
                    .iter()
                    .copied()
                    .map(LinearMemoryLayout::value_type),
            );
            signature_types.push(DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Func {
                    parameters,
                    results: vec![LinearMemoryLayout::value_type(signature.result)],
                },
            });
        }
        Ok(LinearMemoryLayout {
            representations,
            signatures,
            types: if signature_types.is_empty() {
                Vec::new()
            } else {
                vec![crate::types::RecGroup(signature_types)]
            },
            next_offset: offset,
        })
    }
}

fn plan_representation(
    representation: &Representation,
    table: &crate::cc::RepresentationTable,
) -> Result<(LinearRepresentation, u32), LayoutError> {
    match representation {
        Representation::Box { value } => {
            validate_value(value, table)?;
            let size = value_size(value);
            let alignment = value_alignment(value);
            Ok((
                LinearRepresentation::Box {
                    value: *value,
                    size,
                    alignment,
                },
                size,
            ))
        }
        Representation::Product { fields } => {
            let (fields, size, alignment) = plan_fields(fields, table)?;
            Ok((
                LinearRepresentation::Product {
                    fields,
                    size,
                    alignment,
                },
                size,
            ))
        }
        Representation::Variant { cases } => {
            let mut planned_cases = Vec::with_capacity(cases.len());
            let mut size = 4;
            let mut case_alignment = 4;
            for case in cases {
                let (fields, case_size, field_alignment) = plan_fields_at(&case.fields, table, 4)?;
                size = size.max(case_size);
                case_alignment = case_alignment.max(field_alignment);
                planned_cases.push((case.tag, fields));
            }
            let alignment = case_alignment.max(4);
            size = align(size, alignment);
            Ok((
                LinearRepresentation::Variant {
                    tag_size: 4,
                    cases: planned_cases,
                    size,
                    alignment,
                },
                size,
            ))
        }
        Representation::Array { element } => {
            validate_value(element, table)?;
            let stride = value_size(element);
            let alignment = value_alignment(element);
            let element_offset = align(4, alignment);
            Ok((
                LinearRepresentation::Array {
                    element: *element,
                    stride,
                    element_offset,
                    alignment,
                },
                element_offset,
            ))
        }
    }
}

fn plan_fields(
    fields: &[ValueShape],
    table: &crate::cc::RepresentationTable,
) -> Result<(Vec<LinearField>, u32, u32), LayoutError> {
    plan_fields_at(fields, table, 0)
}

fn plan_fields_at(
    fields: &[ValueShape],
    table: &crate::cc::RepresentationTable,
    start: u32,
) -> Result<(Vec<LinearField>, u32, u32), LayoutError> {
    let mut offset = start;
    let mut object_alignment = 4;
    let mut planned = Vec::with_capacity(fields.len());
    for value in fields {
        validate_value(value, table)?;
        let size = value_size(value);
        let field_alignment = value_alignment(value);
        object_alignment = object_alignment.max(field_alignment);
        offset = align(offset, field_alignment);
        planned.push(LinearField {
            offset,
            value: *value,
        });
        offset = offset.saturating_add(size);
    }
    Ok((planned, align(offset, object_alignment), object_alignment))
}

fn validate_value(
    value: &ValueShape,
    table: &crate::cc::RepresentationTable,
) -> Result<(), LayoutError> {
    if let ValueShape::Reference(reference) = value {
        match reference.heap {
            crate::cc::RefShape::Repr(id) if table.representation(id).is_none() => {
                return Err(LayoutError::UnknownRepresentation);
            }
            crate::cc::RefShape::Closure(id) if table.signature(id).is_none() => {
                return Err(LayoutError::UnknownSignature);
            }
            _ => {}
        }
    }
    Ok(())
}

fn value_size(value: &ValueShape) -> u32 {
    match value {
        ValueShape::Number => 8,
        ValueShape::Integer | ValueShape::Boolean | ValueShape::Reference(_) => 4,
    }
}

fn value_alignment(value: &ValueShape) -> u32 {
    if value_size(value) >= 8 { 8 } else { 4 }
}

fn align(offset: u32, alignment: u32) -> u32 {
    let remainder = offset % alignment;
    if remainder == 0 {
        offset
    } else {
        offset + alignment - remainder
    }
}

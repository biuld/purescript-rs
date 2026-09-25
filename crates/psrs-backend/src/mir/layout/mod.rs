//! Concrete target layout planning for CC representation handles.

use super::reachable::ReachableHandles;
use crate::TargetCapabilities;
use crate::cc::Module as CcModule;
use crate::cc::{
    RefShape as CcRefShape, Reference as CcReference, ReprId, Representation, RepresentationTable,
    SignatureId, ValueShape as CcValueShape,
};
use crate::types::{
    CompositeType, DefinedType, DefinedTypeId, FieldType, HeapType, RecGroup, RefType, StorageType,
    ValueType,
};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(super) struct PlannedLayout {
    pub(super) types: Vec<RecGroup>,
    repr_indices: HashMap<ReprId, DefinedTypeId>,
    product_fields: HashMap<DefinedTypeId, Vec<CcValueShape>>,
    variant_indices: HashMap<(ReprId, u32), DefinedTypeId>,
    signature_indices: HashMap<SignatureId, DefinedTypeId>,
    closure_index: Option<DefinedTypeId>,
    capture_array_index: Option<DefinedTypeId>,
    boxed_integer_index: Option<DefinedTypeId>,
    boxed_number_index: Option<DefinedTypeId>,
}

impl PlannedLayout {
    #[cfg(test)]
    pub(super) fn plan(
        table: &RepresentationTable,
        target: TargetCapabilities,
    ) -> Result<Self, LayoutError> {
        let repr_ids = (0..table.representations.len())
            .map(|index| ReprId(index as u32))
            .collect::<Vec<_>>();
        let signature_ids = (0..table.signatures.len())
            .map(|index| SignatureId(index as u32))
            .collect::<Vec<_>>();
        Self::plan_selected(table, target, &repr_ids, &signature_ids)
    }

    /// Plans only the representation and signature requirements reachable from
    /// the CC module. P9 owns this reachability decision because it is the
    /// first stage that turns abstract handles into concrete target types.
    pub(super) fn plan_module(
        module: &CcModule,
        target: TargetCapabilities,
    ) -> Result<Self, LayoutError> {
        let reachable = ReachableHandles::from_module(module)?;
        Self::plan_selected(
            &module.representations,
            target,
            &reachable.representations,
            &reachable.signatures,
        )
    }

    fn plan_selected(
        table: &RepresentationTable,
        target: TargetCapabilities,
        repr_ids: &[ReprId],
        signature_ids: &[SignatureId],
    ) -> Result<Self, LayoutError> {
        validate_selected(table, repr_ids, signature_ids)?;
        if !repr_ids.is_empty() && (!target.reference_types || !target.gc) {
            return Err(LayoutError::UnsupportedGcTarget);
        }
        if !signature_ids.is_empty()
            && (!target.reference_types || !target.function_references || !target.gc)
        {
            return Err(LayoutError::UnsupportedClosureTarget);
        }
        let mut repr_indices = HashMap::new();
        let mut product_fields = HashMap::new();
        let mut definitions = Vec::with_capacity(repr_ids.len());
        for (index, id) in repr_ids.iter().enumerate() {
            repr_indices.insert(*id, DefinedTypeId(index as u32));
            if let Some(Representation::Product { fields }) = table.representation(*id) {
                product_fields.insert(DefinedTypeId(index as u32), fields.clone());
            }
            definitions.push(DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Struct(Vec::new()),
            });
        }
        let mut variant_indices = HashMap::new();
        let mut variant_cases = Vec::new();
        for id in repr_ids {
            if let Some(Representation::Variant { cases }) = table.representation(*id) {
                let supertype = repr_indices[id];
                for case in cases {
                    let index = DefinedTypeId(definitions.len() as u32);
                    variant_indices.insert((*id, case.tag), index);
                    variant_cases.push((index, case.fields.clone()));
                    definitions.push(DefinedType {
                        final_type: true,
                        supertype: Some(supertype),
                        composite: CompositeType::Struct(Vec::new()),
                    });
                }
            }
        }
        let (closure_index, capture_array_index) = if signature_ids.is_empty() {
            (None, None)
        } else {
            let capture_array_index = DefinedTypeId(definitions.len() as u32);
            definitions.push(DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Array(FieldType {
                    storage: StorageType::Ref(RefType {
                        nullable: true,
                        heap: HeapType::Eq,
                    }),
                    mutable: true,
                }),
            });
            let closure_index = DefinedTypeId(definitions.len() as u32);
            definitions.push(DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Struct(vec![
                    FieldType {
                        storage: StorageType::Ref(RefType {
                            nullable: false,
                            heap: HeapType::Func,
                        }),
                        mutable: false,
                    },
                    FieldType {
                        storage: StorageType::Ref(RefType {
                            nullable: false,
                            heap: HeapType::Index(capture_array_index),
                        }),
                        mutable: false,
                    },
                ]),
            });
            (Some(closure_index), Some(capture_array_index))
        };
        let boxed_number_index = repr_ids
            .iter()
            .position(|id| {
                matches!(
                    table.representation(*id),
                    Some(Representation::Box {
                        value: CcValueShape::Number
                    })
                )
            })
            .map(|index| DefinedTypeId(index as u32));
        let boxed_integer_index = repr_ids
            .iter()
            .position(|id| {
                matches!(
                    table.representation(*id),
                    Some(Representation::Box {
                        value: CcValueShape::Integer
                    })
                )
            })
            .map(|index| DefinedTypeId(index as u32));
        for (index, id) in repr_ids.iter().enumerate() {
            let representation = table
                .representation(*id)
                .ok_or(LayoutError::UnknownRepresentation)?;
            let composite = match representation {
                Representation::Box { value } => CompositeType::Struct(vec![FieldType {
                    storage: storage_type(value, &repr_indices, closure_index)?,
                    mutable: false,
                }]),
                Representation::Product { fields } => CompositeType::Struct(
                    fields
                        .iter()
                        .map(|value| {
                            Ok(FieldType {
                                storage: storage_type(value, &repr_indices, closure_index)?,
                                mutable: false,
                            })
                        })
                        .collect::<Result<Vec<_>, LayoutError>>()?,
                ),
                Representation::Variant { cases } => {
                    if cases.is_empty() {
                        return Err(LayoutError::IncompatibleVariant);
                    }
                    definitions[index].final_type = false;
                    CompositeType::Struct(vec![FieldType {
                        storage: StorageType::I32,
                        mutable: false,
                    }])
                }
                Representation::Array { element } => CompositeType::Array(FieldType {
                    storage: array_storage_type(element, &repr_indices, closure_index)?,
                    mutable: true,
                }),
            };
            definitions[index].composite = composite;
        }
        for (index, fields) in variant_cases {
            let mut concrete = vec![FieldType {
                storage: StorageType::I32,
                mutable: false,
            }];
            concrete.extend(
                fields
                    .iter()
                    .map(|value| {
                        Ok(FieldType {
                            storage: storage_type(value, &repr_indices, closure_index)?,
                            mutable: false,
                        })
                    })
                    .collect::<Result<Vec<_>, LayoutError>>()?,
            );
            definitions[index.0 as usize].composite = CompositeType::Struct(concrete);
        }

        let mut signature_indices = HashMap::new();
        for id in signature_ids {
            let signature = table.signature(*id).ok_or(LayoutError::UnknownSignature)?;
            let index = DefinedTypeId(definitions.len() as u32);
            signature_indices.insert(*id, index);
            let mut parameters = vec![ValueType::Ref(RefType {
                nullable: false,
                heap: HeapType::Struct,
            })];
            parameters.extend(
                signature
                    .parameters
                    .iter()
                    .map(|value| value_type(value, &repr_indices, closure_index))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            definitions.push(DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Func {
                    parameters,
                    results: vec![value_type(&signature.result, &repr_indices, closure_index)?],
                },
            });
        }

        Ok(Self {
            types: if definitions.is_empty() {
                Vec::new()
            } else {
                vec![RecGroup(definitions)]
            },
            repr_indices,
            product_fields,
            variant_indices,
            signature_indices,
            closure_index,
            capture_array_index,
            boxed_integer_index,
            boxed_number_index,
        })
    }
    pub(super) fn repr_index(&self, id: ReprId) -> Result<DefinedTypeId, LayoutError> {
        self.repr_indices
            .get(&id)
            .copied()
            .ok_or(LayoutError::UnknownRepresentation)
    }
    pub(super) fn product_field(
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
    pub(super) fn variant_index(
        &self,
        id: ReprId,
        case: u32,
    ) -> Result<DefinedTypeId, LayoutError> {
        self.variant_indices
            .get(&(id, case))
            .copied()
            .ok_or(LayoutError::UnknownField)
    }
    pub(super) fn signature_index(&self, id: SignatureId) -> Result<DefinedTypeId, LayoutError> {
        self.signature_indices
            .get(&id)
            .copied()
            .ok_or(LayoutError::UnknownSignature)
    }

    pub(super) fn closure_layout(&self) -> Result<(DefinedTypeId, DefinedTypeId), LayoutError> {
        self.closure_index
            .zip(self.capture_array_index)
            .ok_or(LayoutError::UnknownClosureLayout)
    }

    pub(super) fn boxed_number_index(&self) -> Option<DefinedTypeId> {
        self.boxed_number_index
    }

    pub(super) fn boxed_integer_index(&self) -> Option<DefinedTypeId> {
        self.boxed_integer_index
    }

    pub(super) fn value_type(&self, value: &CcValueShape) -> Result<ValueType, LayoutError> {
        value_type(value, &self.repr_indices, self.closure_index)
    }

    pub(super) fn reference(&self, reference: &CcReference) -> Result<RefType, LayoutError> {
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

fn validate_selected(
    table: &RepresentationTable,
    repr_ids: &[ReprId],
    signature_ids: &[SignatureId],
) -> Result<(), LayoutError> {
    for id in repr_ids {
        let representation = table
            .representation(*id)
            .ok_or(LayoutError::UnknownRepresentation)?;
        validate_representation(representation, table)?;
    }
    for id in signature_ids {
        let signature = table.signature(*id).ok_or(LayoutError::UnknownSignature)?;
        for parameter in &signature.parameters {
            validate_value_shape(parameter, table)?;
        }
        validate_value_shape(&signature.result, table)?;
    }
    Ok(())
}

fn validate_representation(
    representation: &Representation,
    table: &RepresentationTable,
) -> Result<(), LayoutError> {
    match representation {
        Representation::Box { value } | Representation::Array { element: value } => {
            validate_value_shape(value, table)?;
        }
        Representation::Product { fields } => {
            for field in fields {
                validate_value_shape(field, table)?;
            }
        }
        Representation::Variant { cases } => {
            for case in cases {
                for field in &case.fields {
                    validate_value_shape(field, table)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_value_shape(
    value: &CcValueShape,
    table: &RepresentationTable,
) -> Result<(), LayoutError> {
    let CcValueShape::Reference(reference) = value else {
        return Ok(());
    };
    match reference.heap {
        CcRefShape::Repr(id) if table.representation(id).is_none() => {
            Err(LayoutError::UnknownRepresentation)
        }
        CcRefShape::Closure(id) if table.signature(id).is_none() => {
            Err(LayoutError::UnknownSignature)
        }
        _ => Ok(()),
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum LayoutError {
    UnknownRepresentation,
    UnknownSignature,
    UnknownField,
    MissingIntegerBox,
    MissingNumberBox,
    UnknownClosureLayout,
    UnsupportedGcTarget,
    UnsupportedClosureTarget,
    IncompatibleVariant,
    UnsupportedValue,
}

fn value_type(
    value: &CcValueShape,
    repr_indices: &HashMap<ReprId, DefinedTypeId>,
    closure_index: Option<DefinedTypeId>,
) -> Result<ValueType, LayoutError> {
    Ok(match value {
        CcValueShape::Integer => ValueType::I32,
        CcValueShape::Boolean => ValueType::Boolean,
        CcValueShape::Number => ValueType::F64,
        CcValueShape::Reference(reference) => ValueType::Ref(RefType {
            nullable: reference.nullable,
            heap: match reference.heap {
                CcRefShape::Repr(id) => HeapType::Index(
                    repr_indices
                        .get(&id)
                        .copied()
                        .ok_or(LayoutError::UnknownRepresentation)?,
                ),
                CcRefShape::Aggregate => HeapType::Struct,
                CcRefShape::Closure(_) => {
                    HeapType::Index(closure_index.ok_or(LayoutError::UnknownRepresentation)?)
                }
                CcRefShape::Erased => HeapType::Eq,
            },
        }),
    })
}

fn storage_type(
    value: &CcValueShape,
    repr_indices: &HashMap<ReprId, DefinedTypeId>,
    closure_index: Option<DefinedTypeId>,
) -> Result<StorageType, LayoutError> {
    Ok(match value_type(value, repr_indices, closure_index)? {
        ValueType::I32 | ValueType::Boolean => StorageType::I32,
        ValueType::F64 => StorageType::F64,
        ValueType::Ref(reference) => StorageType::Ref(reference),
        _ => return Err(LayoutError::UnsupportedValue),
    })
}

fn array_storage_type(
    value: &CcValueShape,
    repr_indices: &HashMap<ReprId, DefinedTypeId>,
    closure_index: Option<DefinedTypeId>,
) -> Result<StorageType, LayoutError> {
    let mut storage = storage_type(value, repr_indices, closure_index)?;
    if let StorageType::Ref(reference) = &mut storage {
        // Dynamic clones use `array.new_default`, so reference slots must be
        // nullable while the fresh array is being initialized by `array.copy`.
        reference.nullable = true;
    }
    Ok(storage)
}

#[cfg(test)]
mod tests;

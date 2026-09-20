//! Concrete target layout planning for CC representation handles.

use crate::TargetCapabilities;
use crate::cc::{
    RefShape as CcRefShape, Reference as CcReference, ReprId, Representation, RepresentationTable,
    SignatureId, ValueShape as CcValueShape,
};
use crate::types::{
    CompositeType, DefinedType, FieldType, HeapType, RecGroup, RefType, StorageType, ValueType,
};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(super) struct PlannedLayout {
    pub(super) types: Vec<RecGroup>,
    repr_indices: HashMap<ReprId, u32>,
    signature_indices: HashMap<SignatureId, u32>,
    closure_index: Option<u32>,
    capture_array_index: Option<u32>,
    boxed_number_index: Option<u32>,
}

impl PlannedLayout {
    pub(super) fn plan(
        table: &RepresentationTable,
        target: TargetCapabilities,
    ) -> Result<Self, LayoutError> {
        validate_table(table)?;
        if !table.representations.is_empty() && (!target.reference_types || !target.gc) {
            return Err(LayoutError::UnsupportedGcTarget);
        }
        if !table.signatures.is_empty()
            && (!target.reference_types || !target.function_references || !target.gc)
        {
            return Err(LayoutError::UnsupportedClosureTarget);
        }
        let mut repr_indices = HashMap::new();
        let mut definitions = Vec::with_capacity(table.representations.len());
        for (index, _) in table.representations.iter().enumerate() {
            repr_indices.insert(ReprId(index as u32), index as u32);
            definitions.push(DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Struct(Vec::new()),
            });
        }
        let (closure_index, capture_array_index) = if table.signatures.is_empty() {
            (None, None)
        } else {
            let capture_array_index = definitions.len() as u32;
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
            let closure_index = definitions.len() as u32;
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
        let boxed_number_index = table
            .representations
            .iter()
            .position(|representation| {
                matches!(
                    representation,
                    Representation::Box {
                        value: CcValueShape::Number
                    }
                )
            })
            .map(|index| index as u32);
        for (index, representation) in table.representations.iter().enumerate() {
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
                    let first = cases
                        .first()
                        .map(|case| case.fields.as_slice())
                        .unwrap_or(&[]);
                    for case in cases.iter().skip(1) {
                        let same_shape = case.fields.len() == first.len()
                            && case
                                .fields
                                .iter()
                                .zip(first)
                                .map(|(left, right)| {
                                    Ok((
                                        storage_type(left, &repr_indices, closure_index)?,
                                        storage_type(right, &repr_indices, closure_index)?,
                                    ))
                                })
                                .collect::<Result<Vec<_>, LayoutError>>()?
                                .iter()
                                .all(|(left, right)| left == right);
                        if !same_shape {
                            return Err(LayoutError::IncompatibleVariant);
                        }
                    }
                    let fields = cases
                        .first()
                        .map(|case| case.fields.clone())
                        .unwrap_or_default();
                    CompositeType::Struct(
                        std::iter::once(Ok(FieldType {
                            storage: StorageType::I32,
                            mutable: false,
                        }))
                        .chain(fields.iter().map(|value| {
                            Ok(FieldType {
                                storage: storage_type(value, &repr_indices, closure_index)?,
                                mutable: false,
                            })
                        }))
                        .collect::<Result<Vec<_>, LayoutError>>()?,
                    )
                }
                Representation::Array { element } => CompositeType::Array(FieldType {
                    storage: storage_type(element, &repr_indices, closure_index)?,
                    mutable: true,
                }),
            };
            definitions[index].composite = composite;
        }

        let mut signature_indices = HashMap::new();
        for (id, signature) in table.signatures.iter().enumerate() {
            let index = definitions.len() as u32;
            signature_indices.insert(SignatureId(id as u32), index);
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
            signature_indices,
            closure_index,
            capture_array_index,
            boxed_number_index,
        })
    }

    pub(super) fn repr_index(&self, id: ReprId) -> Result<u32, LayoutError> {
        self.repr_indices
            .get(&id)
            .copied()
            .ok_or(LayoutError::UnknownRepresentation)
    }

    pub(super) fn signature_index(&self, id: SignatureId) -> Result<u32, LayoutError> {
        self.signature_indices
            .get(&id)
            .copied()
            .ok_or(LayoutError::UnknownSignature)
    }

    pub(super) fn closure_layout(&self) -> Result<(u32, u32), LayoutError> {
        self.closure_index
            .zip(self.capture_array_index)
            .ok_or(LayoutError::UnknownClosureLayout)
    }

    pub(super) fn boxed_number_index(&self) -> Option<u32> {
        self.boxed_number_index
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

fn validate_table(table: &RepresentationTable) -> Result<(), LayoutError> {
    for representation in &table.representations {
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
    }
    for signature in &table.signatures {
        for parameter in &signature.parameters {
            validate_value_shape(parameter, table)?;
        }
        validate_value_shape(&signature.result, table)?;
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
    UnknownClosureLayout,
    UnsupportedGcTarget,
    UnsupportedClosureTarget,
    IncompatibleVariant,
    UnsupportedValue,
}

fn value_type(
    value: &CcValueShape,
    repr_indices: &HashMap<ReprId, u32>,
    closure_index: Option<u32>,
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
    repr_indices: &HashMap<ReprId, u32>,
    closure_index: Option<u32>,
) -> Result<StorageType, LayoutError> {
    Ok(match value_type(value, repr_indices, closure_index)? {
        ValueType::I32 | ValueType::Boolean => StorageType::I32,
        ValueType::F64 => StorageType::F64,
        ValueType::Ref(reference) => StorageType::Ref(reference),
        _ => return Err(LayoutError::UnsupportedValue),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc::{Reference, Signature};

    #[test]
    fn planner_owns_the_gc_closure_and_capture_layouts() {
        let mut table = RepresentationTable::default();
        table.add_signature(Signature {
            parameters: vec![CcValueShape::Integer],
            result: CcValueShape::Integer,
        });

        let layout = PlannedLayout::plan(&table, TargetCapabilities::default())
            .expect("planning a closure signature");
        let definitions = &layout.types[0].0;

        assert!(matches!(definitions[0].composite, CompositeType::Array(_)));
        assert!(matches!(definitions[1].composite, CompositeType::Struct(_)));
        assert!(matches!(
            definitions[2].composite,
            CompositeType::Func { .. }
        ));
        assert_eq!(layout.closure_layout().unwrap(), (1, 0));
    }

    #[test]
    fn planner_rejects_a_dangling_closure_signature() {
        let table = RepresentationTable {
            representations: vec![Representation::Product {
                fields: vec![CcValueShape::Reference(Reference {
                    nullable: false,
                    heap: CcRefShape::Closure(SignatureId(0)),
                })],
            }],
            signatures: Vec::new(),
        };

        assert!(matches!(
            PlannedLayout::plan(&table, TargetCapabilities::default()),
            Err(LayoutError::UnknownSignature)
        ));
    }

    #[test]
    fn gc_planner_rejects_an_mvp_only_target() {
        let table = RepresentationTable {
            representations: vec![Representation::Product { fields: Vec::new() }],
            signatures: Vec::new(),
        };

        assert!(matches!(
            PlannedLayout::plan(&table, TargetCapabilities::wasm_mvp()),
            Err(LayoutError::UnsupportedGcTarget)
        ));
    }
}

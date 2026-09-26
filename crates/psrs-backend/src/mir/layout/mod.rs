//! Concrete target layout planning for CC representation handles.

use super::reachable::ReachableHandles;
use crate::TargetCapabilities;
use crate::cc::Module as CcModule;
use crate::cc::{
    RefShape as CcRefShape, ReprId, Representation, RepresentationTable, SignatureId,
    ValueShape as CcValueShape,
};
use crate::types::{
    CompositeType, DefinedType, DefinedTypeId, FieldType, HeapType, RecGroup, RefType, StorageType,
    ValueType,
};
use std::collections::HashMap;

mod accessors;
mod validate;
use validate::validate_selected;

#[derive(Clone, Debug)]
pub(super) struct PlannedLayout {
    pub(super) types: Vec<RecGroup>,
    repr_indices: HashMap<ReprId, DefinedTypeId>,
    product_fields: HashMap<DefinedTypeId, Vec<CcValueShape>>,
    /// Record products by representation handle, with their canonical labels,
    /// so the WIT adapter can project fields by WIT name.
    wit_products: HashMap<ReprId, (Vec<CcValueShape>, Vec<String>)>,
    array_elements: HashMap<ReprId, CcValueShape>,
    variant_indices: HashMap<(ReprId, u32), DefinedTypeId>,
    signature_indices: HashMap<SignatureId, DefinedTypeId>,
    closure_index: Option<DefinedTypeId>,
    capture_array_index: Option<DefinedTypeId>,
    boxed_integer_index: Option<DefinedTypeId>,
    boxed_number_index: Option<DefinedTypeId>,
    string_index: Option<DefinedTypeId>,
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
        let needs_string = repr_ids.iter().any(|id| match table.representation(*id) {
            Some(Representation::Box { value })
            | Some(Representation::Array { element: value }) => *value == CcValueShape::String,
            Some(Representation::Product { fields }) => fields.contains(&CcValueShape::String),
            Some(Representation::Variant { cases }) => cases
                .iter()
                .any(|case| case.fields.contains(&CcValueShape::String)),
            None => false,
        }) || signature_ids.iter().any(|id| {
            table.signature(*id).is_some_and(|signature| {
                signature.parameters.contains(&CcValueShape::String)
                    || signature.result == CcValueShape::String
            })
        });
        Self::plan_selected(table, target, &repr_ids, &signature_ids, needs_string)
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
            reachable.needs_string,
        )
    }

    fn plan_selected(
        table: &RepresentationTable,
        target: TargetCapabilities,
        repr_ids: &[ReprId],
        signature_ids: &[SignatureId],
        needs_string: bool,
    ) -> Result<Self, LayoutError> {
        validate_selected(table, repr_ids, signature_ids)?;
        if (!repr_ids.is_empty() || needs_string) && (!target.reference_types || !target.gc) {
            return Err(LayoutError::UnsupportedGcTarget);
        }
        if !signature_ids.is_empty()
            && (!target.reference_types || !target.function_references || !target.gc)
        {
            return Err(LayoutError::UnsupportedClosureTarget);
        }
        let mut repr_indices = HashMap::new();
        let mut product_fields = HashMap::new();
        let mut wit_products = HashMap::new();
        let mut array_elements = HashMap::new();
        let mut definitions = Vec::with_capacity(repr_ids.len() + 1);
        // The GC string is `(array (mut i16))`: its length is the UTF-16 code
        // unit count, matching PureScript/JS `String` semantics. Reserve it at
        // index 0 so every other concrete type keeps a stable offset.
        let string_index = if needs_string {
            definitions.push(DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Array(FieldType {
                    storage: StorageType::I16,
                    mutable: true,
                }),
            });
            Some(DefinedTypeId(0))
        } else {
            None
        };
        let string_offset = u32::from(needs_string);
        for (index, id) in repr_ids.iter().enumerate() {
            let index = index as u32 + string_offset;
            repr_indices.insert(*id, DefinedTypeId(index));
            if let Some(Representation::Product { fields }) = table.representation(*id) {
                product_fields.insert(DefinedTypeId(index), fields.clone());
            }
            if let Some(Representation::Array { element }) = table.representation(*id) {
                array_elements.insert(*id, *element);
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
            .map(|index| DefinedTypeId(index as u32 + string_offset));
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
            .map(|index| DefinedTypeId(index as u32 + string_offset));
        for (index, id) in repr_ids.iter().enumerate() {
            let index = index as u32 + string_offset;
            let representation = table
                .representation(*id)
                .ok_or(LayoutError::UnknownRepresentation)?;
            let composite = match representation {
                Representation::Box { value } => CompositeType::Struct(vec![FieldType {
                    storage: storage_type(value, &repr_indices, closure_index, string_index)?,
                    mutable: false,
                }]),
                Representation::Product { fields } => CompositeType::Struct(
                    fields
                        .iter()
                        .map(|value| {
                            Ok(FieldType {
                                storage: storage_type(
                                    value,
                                    &repr_indices,
                                    closure_index,
                                    string_index,
                                )?,
                                mutable: false,
                            })
                        })
                        .collect::<Result<Vec<_>, LayoutError>>()?,
                ),
                Representation::Variant { cases } => {
                    if cases.is_empty() {
                        return Err(LayoutError::IncompatibleVariant);
                    }
                    definitions[index as usize].final_type = false;
                    CompositeType::Struct(vec![FieldType {
                        storage: StorageType::I32,
                        mutable: false,
                    }])
                }
                Representation::Array { element } => CompositeType::Array(FieldType {
                    storage: array_storage_type(
                        element,
                        &repr_indices,
                        closure_index,
                        string_index,
                    )?,
                    mutable: true,
                }),
            };
            definitions[index as usize].composite = composite;
            if let Representation::Product { fields } = representation {
                let labels = table.product_labels.get(id).cloned().unwrap_or_default();
                wit_products.insert(*id, (fields.clone(), labels));
            }
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
                            storage: storage_type(
                                value,
                                &repr_indices,
                                closure_index,
                                string_index,
                            )?,
                            mutable: false,
                        })
                    })
                    .collect::<Result<Vec<_>, LayoutError>>()?,
            );
            definitions[index.0 as usize].composite = CompositeType::Struct(concrete);
        }

        // Distinct CC signatures can lower to the same concrete Wasm function
        // type (for example `Boolean -> Boolean` and `Integer -> Integer`, both
        // `i32 -> i32`). A closure stores an abstract `funcref` and the call site
        // casts it to the concrete signature type, so two such signatures must
        // share one type index or the cast traps. Share the definition whenever
        // the lowered parameter and result types are identical.
        let mut signature_indices = HashMap::new();
        let mut signature_types = HashMap::<(Vec<ValueType>, Vec<ValueType>), DefinedTypeId>::new();
        for id in signature_ids {
            let signature = table.signature(*id).ok_or(LayoutError::UnknownSignature)?;
            let mut parameters = vec![ValueType::Ref(RefType {
                nullable: false,
                heap: HeapType::Struct,
            })];
            parameters.extend(
                signature
                    .parameters
                    .iter()
                    .map(|value| value_type(value, &repr_indices, closure_index, string_index))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            let results = [value_type(
                &signature.result,
                &repr_indices,
                closure_index,
                string_index,
            )?];
            let key = (
                parameters
                    .iter()
                    .copied()
                    .map(concrete_value_type)
                    .collect(),
                results.iter().copied().map(concrete_value_type).collect(),
            );
            let index = match signature_types.get(&key) {
                Some(index) => *index,
                None => {
                    let index = DefinedTypeId(definitions.len() as u32);
                    definitions.push(DefinedType {
                        final_type: true,
                        supertype: None,
                        composite: CompositeType::Func {
                            parameters: key.0.clone(),
                            results: key.1.clone(),
                        },
                    });
                    signature_types.insert(key, index);
                    index
                }
            };
            signature_indices.insert(*id, index);
        }

        Ok(Self {
            types: if definitions.is_empty() {
                Vec::new()
            } else {
                vec![RecGroup(definitions)]
            },
            repr_indices,
            product_fields,
            wit_products,
            array_elements,
            variant_indices,
            signature_indices,
            closure_index,
            capture_array_index,
            boxed_integer_index,
            boxed_number_index,
            string_index,
        })
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

/// The concrete Wasm value type behind a MIR value type. `Boolean` and `I32`
/// share `i32`, so signatures that differ only there must share a function type.
fn concrete_value_type(value: ValueType) -> ValueType {
    match value {
        ValueType::Boolean => ValueType::I32,
        other => other,
    }
}

fn value_type(
    value: &CcValueShape,
    repr_indices: &HashMap<ReprId, DefinedTypeId>,
    closure_index: Option<DefinedTypeId>,
    string_index: Option<DefinedTypeId>,
) -> Result<ValueType, LayoutError> {
    Ok(match value {
        CcValueShape::Integer => ValueType::I32,
        CcValueShape::Boolean => ValueType::Boolean,
        CcValueShape::Number => ValueType::F64,
        CcValueShape::String => ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(string_index.ok_or(LayoutError::UnknownRepresentation)?),
        }),
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
    string_index: Option<DefinedTypeId>,
) -> Result<StorageType, LayoutError> {
    Ok(
        match value_type(value, repr_indices, closure_index, string_index)? {
            ValueType::I32 | ValueType::Boolean => StorageType::I32,
            ValueType::F64 => StorageType::F64,
            ValueType::Ref(reference) => StorageType::Ref(reference),
            _ => return Err(LayoutError::UnsupportedValue),
        },
    )
}

fn array_storage_type(
    value: &CcValueShape,
    repr_indices: &HashMap<ReprId, DefinedTypeId>,
    closure_index: Option<DefinedTypeId>,
    string_index: Option<DefinedTypeId>,
) -> Result<StorageType, LayoutError> {
    let mut storage = storage_type(value, repr_indices, closure_index, string_index)?;
    if let StorageType::Ref(reference) = &mut storage {
        // Dynamic clones use `array.new_default`, so reference slots must be
        // nullable while the fresh array is being initialized by `array.copy`.
        reference.nullable = true;
    }
    Ok(storage)
}

#[cfg(test)]
mod tests;

//! Discharge record storage views against their owner and actual MIR GC types.
use super::*;
use crate::cc::{Field, GuestLayout, Representation};
use crate::types::{FieldType, RefType};

pub(super) fn verify(module: &Module, defined: &[&DefinedType], errors: &mut Vec<BackendError>) {
    for instruction in module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
    {
        if let Instruction::ListCopy {
            element_guest,
            direction,
            struct_type,
            span,
            ..
        } = instruction
        {
            if let GuestLayout::RecordStorage { storage, .. } = element_guest
                && *direction != crate::mir::ListDirection::Free
                && module
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.repr_index(storage.representation()).ok())
                    != Some(*struct_type)
            {
                errors.extend(mir_error(
                    *span,
                    "MIR record list header disagrees with its owner layout",
                ));
            }
            if let Err(found) = verify_guest(module, defined, element_guest, *span) {
                errors.extend(found);
            }
        }
    }
}

fn verify_guest(
    module: &Module,
    defined: &[&DefinedType],
    guest: &GuestLayout,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    match guest {
        GuestLayout::RecordStorage {
            storage,
            product,
            labels,
            fields,
        } => {
            let layout = module
                .layout
                .as_ref()
                .ok_or_else(|| mir_error(span, "MIR record storage view has no owner layout"))?;
            let table = layout.representation_table();
            if table.record_storage() != Some(*storage)
                || table.verify_record_storage().is_err()
                || table.product_labels(*product) != Some(labels.as_slice())
                || table.representation(*product)
                    != Some(&Representation::Product {
                        fields: fields.iter().map(|field| field.stored).collect(),
                    })
                || labels.len() != fields.len()
                || labels.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err(mir_error(
                    span,
                    "MIR record storage view disagrees with its representation owner",
                ));
            }
            let index = |repr| {
                layout.repr_index(repr).map_err(|_| {
                    mir_error(
                        span,
                        "MIR record storage schema has an unresolved representation",
                    )
                })
            };
            let header = index(storage.representation())?;
            let keys = index(storage.labels())?;
            let values = index(storage.values())?;
            let string = layout
                .string_index()
                .ok_or_else(|| mir_error(span, "MIR record storage labels have no String type"))?;
            let reference = |index, nullable| {
                StorageType::Ref(RefType {
                    nullable,
                    heap: HeapType::Index(index),
                })
            };
            let expected_header = CompositeType::Struct(vec![
                FieldType {
                    storage: reference(keys, false),
                    mutable: false,
                },
                FieldType {
                    storage: reference(values, false),
                    mutable: false,
                },
            ]);
            let expected_keys = CompositeType::Array(FieldType {
                storage: reference(string, true),
                mutable: true,
            });
            let expected_values = CompositeType::Array(FieldType {
                storage: StorageType::Ref(RefType {
                    nullable: true,
                    heap: HeapType::Eq,
                }),
                mutable: true,
            });
            let expected_string = CompositeType::Array(FieldType {
                storage: StorageType::I8,
                mutable: true,
            });
            for (id, expected) in [
                (header, expected_header),
                (keys, expected_keys),
                (values, expected_values),
                (string, expected_string),
            ] {
                if defined.get(id.0 as usize).map(|ty| &ty.composite) != Some(&expected) {
                    return Err(mir_error(
                        span,
                        "MIR record storage schema disagrees with its physical GC definitions",
                    ));
                }
            }
            for field in fields {
                verify_field(
                    module,
                    defined,
                    &Field {
                        value: field.value.clone(),
                        stored: crate::cc::payload::erased_shape(),
                    },
                    span,
                )?;
            }
        }
        GuestLayout::Product { fields, .. } => {
            for field in fields {
                verify_field(module, defined, field, span)?;
            }
        }
        GuestLayout::Variant { cases, .. } => {
            for field in cases.iter().flat_map(|case| &case.fields) {
                verify_field(module, defined, field, span)?;
            }
        }
        GuestLayout::Array { element, .. } => verify_field(module, defined, element, span)?,
        GuestLayout::Scalar { .. } | GuestLayout::Boxed { .. } => {}
    }
    Ok(())
}

fn verify_field(
    module: &Module,
    defined: &[&DefinedType],
    field: &Field,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if field.stored == crate::cc::payload::erased_shape()
        && matches!(
            field.value,
            GuestLayout::Product { .. } | GuestLayout::Array { .. }
        )
    {
        let layout = module
            .layout
            .as_ref()
            .ok_or_else(|| mir_error(span, "MIR erased record/array view has no owner layout"))?;
        let stored = crate::cc::payload::stored_guest_layout(field, layout.representation_table())
            .ok_or_else(|| mir_error(span, "MIR aggregate field has no checked storage view"))?;
        verify_guest(module, defined, &stored, span)
    } else {
        verify_guest(module, defined, &field.value, span)
    }
}

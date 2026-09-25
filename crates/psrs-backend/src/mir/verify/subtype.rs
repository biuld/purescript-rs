use super::util::mir_error;
use crate::BackendError;
use crate::mir::Module;
use crate::types::{CompositeType, DefinedType, HeapType, RefType, StorageType, ValueType};

pub(super) fn verify_subtypes(module: &Module, errors: &mut Vec<BackendError>) {
    let definitions = module
        .types
        .iter()
        .flat_map(|group| group.0.iter())
        .collect::<Vec<_>>();
    for (index, definition) in definitions.iter().enumerate() {
        let Some(supertype_id) = definition.supertype else {
            continue;
        };
        let Some(supertype) = definitions.get(supertype_id.0 as usize) else {
            continue; // The range check in verify_defined_types reports this.
        };
        if supertype.final_type || supertype_id.0 as usize >= index {
            errors.extend(mir_error(
                module.span,
                "MIR supertype must be an earlier non-final defined type",
            ));
            continue;
        }
        if !composite_subtype(definition, supertype, &definitions) {
            errors.extend(mir_error(
                module.span,
                "MIR defined type is incompatible with its declared supertype",
            ));
        }
    }
}

fn composite_subtype(
    subtype: &DefinedType,
    supertype: &DefinedType,
    definitions: &[&DefinedType],
) -> bool {
    match (&subtype.composite, &supertype.composite) {
        (CompositeType::Struct(fields), CompositeType::Struct(parent)) => {
            fields.len() >= parent.len()
                && fields.iter().zip(parent).all(|(field, parent)| {
                    field.mutable == parent.mutable
                        && storage_subtype(
                            field.storage,
                            parent.storage,
                            field.mutable,
                            definitions,
                        )
                })
        }
        (CompositeType::Array(field), CompositeType::Array(parent)) => {
            field.mutable == parent.mutable
                && storage_subtype(field.storage, parent.storage, field.mutable, definitions)
        }
        (
            CompositeType::Func {
                parameters,
                results,
            },
            CompositeType::Func {
                parameters: parent_parameters,
                results: parent_results,
            },
        ) => {
            parameters.len() == parent_parameters.len()
                && results.len() == parent_results.len()
                && parameters
                    .iter()
                    .zip(parent_parameters)
                    .all(|(child, parent)| value_subtype(*parent, *child, definitions))
                && results
                    .iter()
                    .zip(parent_results)
                    .all(|(child, parent)| value_subtype(*child, *parent, definitions))
        }
        _ => false,
    }
}

fn storage_subtype(
    child: StorageType,
    parent: StorageType,
    mutable: bool,
    definitions: &[&DefinedType],
) -> bool {
    if child == parent {
        return true;
    }
    if mutable {
        return false;
    }
    match (child, parent) {
        (StorageType::Ref(child), StorageType::Ref(parent)) => {
            reference_subtype(child, parent, definitions)
        }
        _ => false,
    }
}

fn value_subtype(child: ValueType, parent: ValueType, definitions: &[&DefinedType]) -> bool {
    if child == parent {
        return true;
    }
    match (child, parent) {
        (ValueType::Ref(child), ValueType::Ref(parent)) => {
            reference_subtype(child, parent, definitions)
        }
        _ => false,
    }
}

fn reference_subtype(child: RefType, parent: RefType, definitions: &[&DefinedType]) -> bool {
    (parent.nullable || !child.nullable) && heap_subtype(child.heap, parent.heap, definitions)
}

/// Whether two heap types are related by the Wasm subtype hierarchy in either
/// direction. A `ref.cast`/`ref.test` between unrelated heaps (for example a
/// struct and a function reference) can never succeed and is a compiler bug.
pub(super) fn heap_related(
    child: HeapType,
    parent: HeapType,
    definitions: &[&DefinedType],
) -> bool {
    heap_subtype(child, parent, definitions) || heap_subtype(parent, child, definitions)
}

pub(super) fn heap_subtype(
    child: HeapType,
    parent: HeapType,
    definitions: &[&DefinedType],
) -> bool {
    if child == parent {
        return true;
    }
    match parent {
        HeapType::Any => {
            matches!(
                child,
                HeapType::Eq | HeapType::I31 | HeapType::Struct | HeapType::Array
            ) || matches!(child, HeapType::Index(id) if is_aggregate(id, definitions))
        }
        HeapType::Eq => {
            matches!(child, HeapType::I31 | HeapType::Struct | HeapType::Array)
                || matches!(child, HeapType::Index(id) if is_aggregate(id, definitions))
        }
        HeapType::Struct => matches!(child, HeapType::Index(id) if is_struct(id, definitions)),
        HeapType::Array => matches!(child, HeapType::Index(id) if is_array(id, definitions)),
        HeapType::Func => matches!(child, HeapType::Index(id) if is_function(id, definitions)),
        HeapType::Index(parent_id) => {
            let HeapType::Index(mut child_id) = child else {
                return false;
            };
            while let Some(definition) = definitions.get(child_id.0 as usize) {
                let Some(next) = definition.supertype else {
                    break;
                };
                if next == parent_id {
                    return true;
                }
                if next.0 >= child_id.0 {
                    break;
                }
                child_id = next;
            }
            false
        }
        HeapType::Extern | HeapType::I31 => false,
    }
}

fn is_aggregate(id: crate::types::DefinedTypeId, definitions: &[&DefinedType]) -> bool {
    is_struct(id, definitions) || is_array(id, definitions)
}

fn is_struct(id: crate::types::DefinedTypeId, definitions: &[&DefinedType]) -> bool {
    definitions
        .get(id.0 as usize)
        .is_some_and(|ty| matches!(ty.composite, CompositeType::Struct(_)))
}

fn is_array(id: crate::types::DefinedTypeId, definitions: &[&DefinedType]) -> bool {
    definitions
        .get(id.0 as usize)
        .is_some_and(|ty| matches!(ty.composite, CompositeType::Array(_)))
}

fn is_function(id: crate::types::DefinedTypeId, definitions: &[&DefinedType]) -> bool {
    definitions
        .get(id.0 as usize)
        .is_some_and(|ty| matches!(ty.composite, CompositeType::Func { .. }))
}

//! MIR verification: module-level type and signature setup plus the
//! per-function SSA and instruction checks. See
//! `docs/design/backend/00-ir-boundaries.md`.

use super::{Module, ValueType};
use crate::types::{CompositeType, DefinedType, DefinedTypeId, FunctionId, HeapType, StorageType};
use crate::{BackendError, TargetCapabilities};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod array_map;
mod call;
mod capability;
mod function;
mod instruction;
mod subtype;
mod util;

#[cfg(test)]
mod tests;

use util::{mir_error, value_type};

/// A callable signature used to check call sites. `None` marks a function whose
/// parameters could not be typed, which makes every call to it invalid.
#[derive(Clone)]
struct Signature {
    parameters: Vec<ValueType>,
    result: Option<ValueType>,
}

pub fn verify_module(module: &Module) -> Result<(), Vec<BackendError>> {
    let mut errors = Vec::new();
    verify_defined_types(module, &mut errors);
    let mut signatures = module
        .functions
        .iter()
        .map(|function| {
            let types = function
                .parameters
                .iter()
                .map(|parameter| value_type(function, *parameter))
                .collect::<Option<Vec<_>>>();
            (
                function.symbol,
                types.map(|parameters| Signature {
                    parameters,
                    result: Some(function.result_type),
                }),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut import_symbols = HashSet::new();
    for import in &module.imports {
        if !import_symbols.insert(import.symbol) {
            errors.extend(mir_error(module.span, "MIR import symbols are duplicated"));
        }
        signatures.insert(
            import.symbol,
            Some(Signature {
                parameters: import.parameters.clone(),
                result: import.result,
            }),
        );
    }
    let defined_types = defined_type_count(module);
    let defined = defined_types_list(module);
    let mut function_ids = HashSet::new();
    for (position, function) in module.functions.iter().enumerate() {
        if !function_ids.insert(function.id) || function.id != FunctionId(position as u32) {
            errors.extend(mir_error(
                function.span,
                "MIR function IDs are duplicated or not in module order",
            ));
        }
        for value in &function.values {
            verify_value_type(&value.ty, defined_types, module.span, &mut errors);
        }
        function::verify_function(function, &signatures, &defined).map_err(|function_errors| {
            function_errors
                .into_iter()
                .map(|error| error.with_module(function.symbol.module))
                .collect::<Vec<_>>()
        })?;
    }
    Ok(())
}

/// Verifies structural MIR invariants and legality under the profile used by P9.
pub fn verify_module_with_capabilities(
    module: &Module,
    target: TargetCapabilities,
) -> Result<(), Vec<BackendError>> {
    verify_module(module)?;
    capability::validate_target_capabilities(module, target)
}

fn defined_type_count(module: &Module) -> DefinedTypeId {
    DefinedTypeId(module.types.iter().map(|group| group.0.len() as u32).sum())
}

/// The defined types flattened into the Wasm type index order.
fn defined_types_list(module: &Module) -> Vec<&DefinedType> {
    module
        .types
        .iter()
        .flat_map(|group| group.0.iter())
        .collect()
}

fn verify_defined_types(module: &Module, errors: &mut Vec<BackendError>) {
    let count = defined_type_count(module);
    for group in &module.types {
        for def in &group.0 {
            if let Some(supertype) = def.supertype
                && supertype.0 >= count.0
            {
                errors.extend(mir_error(
                    module.span,
                    "MIR defined type supertype is out of range",
                ));
            }
            match &def.composite {
                CompositeType::Func {
                    parameters,
                    results,
                } => {
                    for ty in parameters.iter().chain(results) {
                        verify_value_type(ty, count, module.span, errors);
                    }
                }
                CompositeType::Struct(fields) => {
                    for field in fields {
                        verify_storage_type(field.storage, count, module.span, errors);
                    }
                }
                CompositeType::Array(field) => {
                    verify_storage_type(field.storage, count, module.span, errors);
                }
            }
        }
    }
    subtype::verify_subtypes(module, errors);
}

fn verify_value_type(
    ty: &ValueType,
    count: DefinedTypeId,
    span: TextRange,
    errors: &mut Vec<BackendError>,
) {
    if let ValueType::Ref(reference) = ty {
        verify_heap(reference.heap, count, span, errors);
    }
}

fn verify_storage_type(
    storage: StorageType,
    count: DefinedTypeId,
    span: TextRange,
    errors: &mut Vec<BackendError>,
) {
    if let StorageType::Ref(reference) = storage {
        verify_heap(reference.heap, count, span, errors);
    }
}

fn verify_heap(
    heap: HeapType,
    count: DefinedTypeId,
    span: TextRange,
    errors: &mut Vec<BackendError>,
) {
    if let HeapType::Index(index) = heap
        && index.0 >= count.0
    {
        errors.extend(mir_error(
            span,
            "MIR defined type reference is out of range",
        ));
    }
}

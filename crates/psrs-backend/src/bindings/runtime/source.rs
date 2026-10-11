//! Source type agreement checked before ABI erasure, including unused imports.
use psrs_core::{Module, Type, TypeConstructor, TypeId};
use psrs_runtime::{StorageOperation, StorageValue};

pub(super) fn validate(
    module: &Module,
    ty: TypeId,
    operation: StorageOperation,
) -> Result<(), String> {
    let (parameters, result) = psrs_core::call_parts(&module.types, ty).map_err(str::to_owned)?;
    let contract = operation.source_contract();
    if parameters.len() != contract.parameters.len() {
        return Err("runtime storage signature has the wrong parameter count".into());
    }
    let mut element = None;
    for (ty, expected) in parameters
        .into_iter()
        .zip(contract.parameters)
        .chain(std::iter::once((result, &contract.payload)))
    {
        let candidate = match expected {
            StorageValue::Int | StorageValue::Unit => {
                let constructor = if *expected == StorageValue::Int {
                    TypeConstructor::Int
                } else {
                    TypeConstructor::Unit
                };
                if module.types.get(ty.0 as usize) != Some(&Type::Constructor(constructor)) {
                    return Err(format!("runtime storage signature expects {expected:?}"));
                }
                continue;
            }
            StorageValue::Array => {
                let Some((TypeConstructor::Array, arguments)) = module.applied_constructor(ty)
                else {
                    return Err("runtime storage signature expects Array".into());
                };
                let [element] = arguments.as_slice() else {
                    return Err("runtime storage Array must have one element type".into());
                };
                *element
            }
            StorageValue::Element => ty,
            StorageValue::Any => continue,
        };
        if element.is_some_and(|previous| !module.types_equivalent(previous, candidate)) {
            return Err("runtime storage signature has inconsistent element types".into());
        }
        element = Some(candidate);
    }
    Ok(())
}

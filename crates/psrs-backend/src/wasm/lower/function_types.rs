//! Final function-type allocation for defined MIR functions.
//!
//! Defined MIR function types occupy the start of the type index space; a
//! function whose signature matches one reuses its index, otherwise a fresh
//! function type is appended after the defined types.

use super::{value_type, wasm_error};
use crate::BackendError;
use crate::mir;
use crate::types::CompositeType;
use crate::wasm::convert::val_type;
use crate::wasm::{FuncType, TypeIndex};
use std::collections::HashMap;
use wasm_encoder::ValType;

pub(super) fn collect_function_types(
    module: &mir::Module,
    defined: u32,
    initial_types: Vec<FuncType>,
) -> Result<(Vec<FuncType>, Vec<TypeIndex>), Vec<BackendError>> {
    let mut types = initial_types;
    let mut indices = HashMap::<(Vec<ValType>, Vec<ValType>), TypeIndex>::new();
    for (index, definition) in module
        .types
        .iter()
        .flat_map(|group| group.0.iter())
        .enumerate()
    {
        let CompositeType::Func {
            parameters,
            results,
        } = &definition.composite
        else {
            continue;
        };
        indices.insert(
            (
                parameters.iter().copied().map(val_type).collect(),
                results.iter().copied().map(val_type).collect(),
            ),
            TypeIndex(index as u32),
        );
    }
    let mut function_types = Vec::with_capacity(module.functions.len());
    for function in &module.functions {
        if function.parameters.len() > function.values.len() {
            return Err(wasm_error(
                function.span,
                "MIR function has more parameters than values",
            ));
        }
        let mut parameters = Vec::with_capacity(function.parameters.len());
        for parameter in &function.parameters {
            let ty = value_type(function, *parameter).ok_or_else(|| {
                wasm_error(function.span, "MIR function parameter has no value type")
            })?;
            parameters.push(val_type(ty));
        }
        let key = (parameters, vec![val_type(function.result_type)]);
        let type_index = match indices.get(&key) {
            Some(index) => *index,
            None => {
                let index = TypeIndex(defined + types.len() as u32);
                types.push(FuncType {
                    parameters: key.0.clone(),
                    results: key.1.clone(),
                });
                indices.insert(key, index);
                index
            }
        };
        function_types.push(type_index);
    }
    Ok((types, function_types))
}

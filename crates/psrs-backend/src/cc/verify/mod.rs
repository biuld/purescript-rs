use super::{Function, Module, Signature};
use crate::BackendError;
use crate::cc::RepresentationTable;
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

mod adaptation;
mod helpers;
mod ops;
mod scalar;
mod variant;
use helpers::{verify_capture_layout, verify_value_shape};
use ops::{verify_assignments, verify_table};

pub(super) fn verify_module(module: &Module) -> Result<(), Vec<BackendError>> {
    verify_table(&module.representations, module.span)?;
    let mut signatures = HashMap::new();
    let mut functions_by_symbol = HashMap::new();
    for function in &module.functions {
        let parameters = function
            .parameters
            .iter()
            .map(|parameter| {
                function
                    .values
                    .iter()
                    .find(|value| value.id == *parameter)
                    .map(|value| value.ty)
                    .ok_or_else(|| {
                        vec![
                            BackendError::new(
                                "P8 CC verification",
                                function.span,
                                "function parameter has no value declaration",
                            )
                            .with_module(function.symbol.module),
                        ]
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if signatures
            .insert(
                function.symbol,
                Signature {
                    parameters,
                    result: function.result_type,
                },
            )
            .is_some()
        {
            return Err(vec![
                BackendError::new(
                    "P8 CC verification",
                    function.span,
                    "CC function symbol is defined more than once",
                )
                .with_module(function.symbol.module),
            ]);
        }
        functions_by_symbol.insert(function.symbol, function);
    }
    for external in &module.externals {
        if let Some(signature) = &external.signature {
            for shape in signature
                .parameters
                .iter()
                .chain(std::iter::once(&signature.result))
            {
                verify_value_shape(shape, &module.representations, module.span).map_err(
                    |errors| {
                        errors
                            .into_iter()
                            .map(|error| error.with_module(external.symbol.module))
                            .collect::<Vec<_>>()
                    },
                )?;
            }
            if signatures
                .insert(external.symbol, signature.clone())
                .is_some()
            {
                return Err(vec![
                    BackendError::new(
                        "P8 CC verification",
                        module.span,
                        "CC external symbol conflicts with another callable symbol",
                    )
                    .with_module(external.symbol.module),
                ]);
            }
        }
    }
    for function in &module.functions {
        verify_function_inner(
            function,
            &signatures,
            &module.representations,
            Some(&functions_by_symbol),
        )
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.with_module(function.symbol.module))
                .collect::<Vec<_>>()
        })?;
    }
    Ok(())
}

/// Verifies a function while it is being built by P8. Module-level checks add
/// target-function and capture compatibility once all generated functions are
/// available.
pub(super) fn verify_function(
    function: &Function,
    signatures: &HashMap<SymbolId, Signature>,
    representations: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    verify_function_inner(function, signatures, representations, None)
}

fn verify_function_inner(
    function: &Function,
    signatures: &HashMap<SymbolId, Signature>,
    representations: &RepresentationTable,
    functions: Option<&HashMap<SymbolId, &Function>>,
) -> Result<(), Vec<BackendError>> {
    let mut declared = HashMap::new();
    for value in &function.values {
        verify_value_shape(&value.ty, representations, function.span)?;
        if declared.insert(value.id, value.ty).is_some() {
            return Err(vec![BackendError::new(
                "P8 CC verification",
                function.span,
                "CC value ID is defined more than once",
            )]);
        }
    }
    let mut parameters = HashSet::new();
    for parameter in &function.parameters {
        if !declared.contains_key(parameter) {
            return Err(vec![BackendError::new(
                "P8 CC verification",
                function.span,
                "function parameter has no value declaration",
            )]);
        }
        if !parameters.insert(*parameter) {
            return Err(vec![BackendError::new(
                "P8 CC verification",
                function.span,
                "function parameter is listed more than once",
            )]);
        }
    }
    verify_capture_layout(function)?;
    let mut available = function.parameters.iter().copied().collect::<HashSet<_>>();
    verify_assignments(
        &function.assignments,
        &mut available,
        &declared,
        signatures,
        representations,
        functions,
        function.span,
    )?;
    if !available.contains(&function.result) {
        return Err(vec![BackendError::new(
            "P8 CC verification",
            function.span,
            "function result is not defined",
        )]);
    }
    if declared.get(&function.result).copied() != Some(function.result_type) {
        return Err(vec![BackendError::new(
            "P8 CC verification",
            function.span,
            "function result type differs from its value declaration",
        )]);
    }
    Ok(())
}

#[cfg(test)]
mod tests;

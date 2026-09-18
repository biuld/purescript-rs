use super::{Assignment, AssignmentKind, Function, Module, Signature, ValueId};
use crate::BackendError;
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

pub(super) fn verify_module(module: &Module) -> Result<(), Vec<BackendError>> {
    let signatures = module
        .functions
        .iter()
        .map(|function| {
            (
                function.symbol,
                Signature {
                    arity: function.parameters.len(),
                    result: function.result_type,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for function in &module.functions {
        verify_function(function, &signatures)?;
    }
    Ok(())
}

pub(super) fn verify_function(
    function: &Function,
    signatures: &HashMap<SymbolId, Signature>,
) -> Result<(), Vec<BackendError>> {
    let mut declared = HashSet::new();
    for value in &function.values {
        if !declared.insert(value.id) {
            return Err(vec![BackendError::new(
                "P8 CC verification",
                function.span,
                "CC value ID is defined more than once",
            )]);
        }
    }
    let mut available = function.parameters.iter().copied().collect::<HashSet<_>>();
    verify_assignments(
        &function.assignments,
        &mut available,
        signatures,
        function.span,
    )?;
    if !available.contains(&function.result) {
        return Err(vec![BackendError::new(
            "P8 CC verification",
            function.span,
            "function result is not defined",
        )]);
    }
    Ok(())
}

fn verify_assignments(
    assignments: &[Assignment],
    available: &mut HashSet<ValueId>,
    signatures: &HashMap<SymbolId, Signature>,
    function_span: TextRange,
) -> Result<(), Vec<BackendError>> {
    for assignment in assignments {
        let mut uses = Vec::new();
        match &assignment.kind {
            AssignmentKind::Constant(_) => {}
            AssignmentKind::Copy(value) => uses.push(*value),
            AssignmentKind::Primitive { left, right, .. } => uses.extend([*left, *right]),
            AssignmentKind::DirectCall {
                function,
                arguments,
            } => {
                let Some(signature) = signatures.get(function) else {
                    return Err(vec![BackendError::new(
                        "P8 CC verification",
                        assignment.span,
                        "direct call references an unknown function",
                    )]);
                };
                if signature.arity != arguments.len() {
                    return Err(vec![BackendError::new(
                        "P8 CC verification",
                        assignment.span,
                        "direct call argument count does not match its signature",
                    )]);
                }
                uses.extend(arguments.iter().copied());
            }
            AssignmentKind::If {
                condition,
                then_assignments,
                then_value,
                else_assignments,
                else_value,
            } => {
                uses.push(*condition);
                for value in &uses {
                    if !available.contains(value) {
                        return Err(undef_error(assignment.span, function_span));
                    }
                }
                let mut then_available = available.clone();
                verify_assignments(
                    then_assignments,
                    &mut then_available,
                    signatures,
                    function_span,
                )?;
                if !then_available.contains(then_value) {
                    return Err(undef_error(assignment.span, function_span));
                }
                let mut else_available = available.clone();
                verify_assignments(
                    else_assignments,
                    &mut else_available,
                    signatures,
                    function_span,
                )?;
                if !else_available.contains(else_value) {
                    return Err(undef_error(assignment.span, function_span));
                }
            }
        }
        if uses.iter().any(|value| !available.contains(value)) {
            return Err(undef_error(assignment.span, function_span));
        }
        if !available.insert(assignment.destination) {
            return Err(vec![BackendError::new(
                "P8 CC verification",
                assignment.span,
                "CC assignment redefines a value",
            )]);
        }
    }
    Ok(())
}

fn undef_error(span: TextRange, fallback: TextRange) -> Vec<BackendError> {
    vec![BackendError::new(
        "P8 CC verification",
        if span == TextRange::new(0, 0) {
            fallback
        } else {
            span
        },
        "CC assignment uses a value before it is defined",
    )]
}

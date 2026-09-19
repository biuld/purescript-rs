use super::{Assignment, AssignmentKind, Function, Module, Signature, ValueId, cc_signature};
use crate::BackendError;
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

pub(super) fn verify_module(module: &Module) -> Result<(), Vec<BackendError>> {
    let mut signatures = module
        .functions
        .iter()
        .map(|function| {
            (
                function.symbol,
                Signature {
                    parameters: function
                        .parameters
                        .iter()
                        .filter_map(|parameter| {
                            function
                                .values
                                .iter()
                                .find(|value| value.id == *parameter)
                                .map(|value| value.ty)
                        })
                        .collect(),
                    result: function.result_type,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for external in &module.externals {
        if let Some(signature) = external.signature.as_ref().and_then(cc_signature) {
            signatures.insert(external.symbol, signature);
        }
    }
    for function in &module.functions {
        verify_function(function, &signatures).map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.with_module(function.symbol.module))
                .collect::<Vec<_>>()
        })?;
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
            AssignmentKind::NumberConstant(_) => {}
            AssignmentKind::StringConstant(_) => {}
            AssignmentKind::FunctionRef { .. } => {}
            AssignmentKind::ClosureGetCapture { closure, .. } => uses.push(*closure),
            AssignmentKind::Primitive { left, right, .. } => uses.extend([*left, *right]),
            AssignmentKind::RefTest { value, .. }
            | AssignmentKind::RefCast { value, .. }
            | AssignmentKind::StructGet { value, .. } => uses.push(*value),
            AssignmentKind::StructNew { arguments, .. } => uses.extend(arguments.iter().copied()),
            AssignmentKind::ArrayNew { elements, .. } => uses.extend(elements.iter().copied()),
            AssignmentKind::ArrayLen { value, .. } => uses.push(*value),
            AssignmentKind::ArrayGet { value, index, .. } => uses.extend([*value, *index]),
            AssignmentKind::ArraySet {
                value,
                index,
                new_value,
                ..
            } => uses.extend([*value, *index, *new_value]),
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
                if signature.parameters.len() != arguments.len() {
                    return Err(vec![BackendError::new(
                        "P8 CC verification",
                        assignment.span,
                        "direct call argument count does not match its signature",
                    )]);
                }
                uses.extend(arguments.iter().copied());
            }
            AssignmentKind::IndirectCall {
                function,
                arguments,
                ..
            } => {
                uses.push(*function);
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
        let preserves_existing_value = matches!(
            &assignment.kind,
            AssignmentKind::ArraySet {
                destination,
                value,
                ..
            } if *destination == *value && *destination == assignment.destination
        );
        if !preserves_existing_value && !available.insert(assignment.destination) {
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

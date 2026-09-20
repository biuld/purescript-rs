use super::{Assignment, AssignmentKind, Function, Module, Signature, ValueId, ValueShape};
use crate::BackendError;
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

pub(super) fn verify_module(module: &Module) -> Result<(), Vec<BackendError>> {
    let mut signatures = HashMap::new();
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
    }
    for external in &module.externals {
        if let Some(signature) = &external.signature
            && signatures
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
    let mut declared = HashMap::new();
    for value in &function.values {
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
    let mut available = function.parameters.iter().copied().collect::<HashSet<_>>();
    verify_assignments(
        &function.assignments,
        &mut available,
        &declared,
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
    if declared.get(&function.result).copied() != Some(function.result_type) {
        return Err(vec![BackendError::new(
            "P8 CC verification",
            function.span,
            "function result type differs from its value declaration",
        )]);
    }
    Ok(())
}

fn verify_assignments(
    assignments: &[Assignment],
    available: &mut HashSet<ValueId>,
    declared: &HashMap<ValueId, ValueShape>,
    signatures: &HashMap<SymbolId, Signature>,
    function_span: TextRange,
) -> Result<(), Vec<BackendError>> {
    for assignment in assignments {
        let mut uses = Vec::new();
        match &assignment.kind {
            AssignmentKind::Constant(_) => {
                if !matches!(
                    declared.get(&assignment.destination),
                    Some(ValueShape::Integer | ValueShape::Boolean)
                ) {
                    return Err(assignment_error(
                        assignment,
                        "integer constant has an incompatible result shape",
                    ));
                }
            }
            AssignmentKind::NumberConstant(_) => {
                if declared.get(&assignment.destination) != Some(&ValueShape::Number) {
                    return Err(assignment_error(
                        assignment,
                        "number constant has an incompatible result shape",
                    ));
                }
            }
            AssignmentKind::StringConstant(_) => {
                if declared.get(&assignment.destination) != Some(&ValueShape::Integer) {
                    return Err(assignment_error(
                        assignment,
                        "string constant has an incompatible result shape",
                    ));
                }
            }
            AssignmentKind::FunctionRef { .. } => {}
            AssignmentKind::ClosureGetCapture { closure, .. } => uses.push(*closure),
            AssignmentKind::Primitive { left, right, .. } => uses.extend([*left, *right]),
            AssignmentKind::RepresentationTest { value, .. }
            | AssignmentKind::RepresentationCast { value, .. }
            | AssignmentKind::ProductGet { value, .. } => uses.push(*value),
            AssignmentKind::ProductNew { arguments, .. } => uses.extend(arguments.iter().copied()),
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
                if arguments
                    .iter()
                    .zip(&signature.parameters)
                    .any(|(argument, expected)| declared.get(argument) != Some(expected))
                {
                    return Err(assignment_error(
                        assignment,
                        "direct call argument shape does not match its signature",
                    ));
                }
                if declared.get(&assignment.destination) != Some(&signature.result) {
                    return Err(assignment_error(
                        assignment,
                        "direct call result shape does not match its signature",
                    ));
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
                    declared,
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
                    declared,
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
        if !declared.contains_key(&assignment.destination) {
            return Err(vec![BackendError::new(
                "P8 CC verification",
                assignment.span,
                "CC assignment destination has no value declaration",
            )]);
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

fn assignment_error(assignment: &Assignment, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new(
        "P8 CC verification",
        assignment.span,
        message,
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc::ValueDecl;
    use psrs_hir::ModuleId;

    fn symbol(index: u32) -> SymbolId {
        SymbolId::new(ModuleId(0), index)
    }

    #[test]
    fn rejects_an_undeclared_parameter() {
        let function = Function {
            symbol: symbol(0),
            name: "invalid".into(),
            parameters: vec![ValueId(0)],
            values: Vec::new(),
            assignments: Vec::new(),
            result: ValueId(0),
            result_type: ValueShape::Integer,
            span: TextRange::new(0, 1),
        };

        assert!(verify_function(&function, &HashMap::new()).is_err());
    }

    #[test]
    fn rejects_a_direct_call_with_the_wrong_result_shape() {
        let callee = symbol(1);
        let destination = ValueId(0);
        let function = Function {
            symbol: symbol(0),
            name: "invalid".into(),
            parameters: Vec::new(),
            values: vec![ValueDecl {
                id: destination,
                ty: ValueShape::Number,
            }],
            assignments: vec![Assignment {
                destination,
                kind: AssignmentKind::DirectCall {
                    function: callee,
                    arguments: Vec::new(),
                },
                span: TextRange::new(0, 1),
            }],
            result: destination,
            result_type: ValueShape::Number,
            span: TextRange::new(0, 1),
        };
        let signatures = HashMap::from([(
            callee,
            Signature {
                parameters: Vec::new(),
                result: ValueShape::Integer,
            },
        )]);

        assert!(verify_function(&function, &signatures).is_err());
    }
}

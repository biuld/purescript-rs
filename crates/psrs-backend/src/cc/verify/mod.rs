use super::{Function, Module, Signature};
use crate::BackendError;
use crate::cc::RepresentationTable;
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

mod helpers;
mod ops;
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
mod tests {
    use super::*;
    use crate::cc::{Assignment, AssignmentKind, Reference, ValueDecl, ValueShape};
    use psrs_hir::ModuleId;
    use psrs_span::TextRange;

    fn symbol(index: u32) -> SymbolId {
        SymbolId::new(ModuleId(0), index)
    }

    fn table() -> RepresentationTable {
        RepresentationTable::default()
    }

    #[test]
    fn rejects_an_undeclared_parameter() {
        let function = Function {
            symbol: symbol(0),
            name: "invalid".into(),
            parameters: vec![super::super::ValueId(0)],
            values: Vec::new(),
            assignments: Vec::new(),
            result: super::super::ValueId(0),
            result_type: ValueShape::Integer,
            span: TextRange::new(0, 1),
        };

        assert!(verify_function(&function, &HashMap::new(), &table()).is_err());
    }

    #[test]
    fn rejects_a_direct_call_with_the_wrong_result_shape() {
        let callee = symbol(1);
        let destination = super::super::ValueId(0);
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

        assert!(verify_function(&function, &signatures, &table()).is_err());
    }

    #[test]
    fn rejects_a_non_boolean_if_condition() {
        let condition = super::super::ValueId(0);
        let result = super::super::ValueId(1);
        let function = Function {
            symbol: symbol(0),
            name: "invalid".into(),
            parameters: vec![condition],
            values: vec![
                ValueDecl {
                    id: condition,
                    ty: ValueShape::Integer,
                },
                ValueDecl {
                    id: result,
                    ty: ValueShape::Integer,
                },
            ],
            assignments: vec![Assignment {
                destination: result,
                kind: AssignmentKind::If {
                    condition,
                    then_assignments: Vec::new(),
                    then_value: condition,
                    else_assignments: Vec::new(),
                    else_value: condition,
                },
                span: TextRange::new(0, 1),
            }],
            result,
            result_type: ValueShape::Integer,
            span: TextRange::new(0, 1),
        };

        assert!(verify_function(&function, &HashMap::new(), &table()).is_err());
    }

    #[test]
    fn rejects_an_array_operation_with_the_wrong_element_shape() {
        let mut representations = table();
        let array = representations.reserve();
        representations.set(
            array,
            super::super::Representation::Array {
                element: ValueShape::Integer,
            },
        );
        let array_value = super::super::ValueId(0);
        let element = super::super::ValueId(1);
        let destination = super::super::ValueId(2);
        let function = Function {
            symbol: symbol(0),
            name: "invalid".into(),
            parameters: vec![array_value, element],
            values: vec![
                ValueDecl {
                    id: array_value,
                    ty: ValueShape::Reference(Reference {
                        nullable: false,
                        heap: super::super::RefShape::Repr(array),
                    }),
                },
                ValueDecl {
                    id: element,
                    ty: ValueShape::Number,
                },
                ValueDecl {
                    id: destination,
                    ty: ValueShape::Reference(Reference {
                        nullable: false,
                        heap: super::super::RefShape::Repr(array),
                    }),
                },
            ],
            assignments: vec![Assignment {
                destination,
                kind: AssignmentKind::ArraySet {
                    destination,
                    representation: array,
                    value: array_value,
                    index: element,
                    new_value: element,
                },
                span: TextRange::new(0, 1),
            }],
            result: destination,
            result_type: ValueShape::Reference(Reference {
                nullable: false,
                heap: super::super::RefShape::Repr(array),
            }),
            span: TextRange::new(0, 1),
        };

        assert!(verify_function(&function, &HashMap::new(), &representations).is_err());
    }

    #[test]
    fn rejects_a_function_reference_with_wrong_captures() {
        let signature = super::super::SignatureId(0);
        let closure = super::super::ValueId(0);
        let target_result = super::super::ValueId(1);
        let target = Function {
            symbol: symbol(1),
            name: "target".into(),
            parameters: vec![closure],
            values: vec![
                ValueDecl {
                    id: closure,
                    ty: super::super::ValueShape::Reference(Reference {
                        nullable: false,
                        heap: super::super::RefShape::Aggregate,
                    }),
                },
                ValueDecl {
                    id: target_result,
                    ty: ValueShape::Integer,
                },
            ],
            assignments: vec![Assignment {
                destination: target_result,
                kind: AssignmentKind::Constant(1),
                span: TextRange::new(0, 1),
            }],
            result: target_result,
            result_type: ValueShape::Integer,
            span: TextRange::new(0, 1),
        };
        let capture = super::super::ValueId(0);
        let function_value = super::super::ValueId(1);
        let caller = Function {
            symbol: symbol(0),
            name: "caller".into(),
            parameters: vec![capture],
            values: vec![
                ValueDecl {
                    id: capture,
                    ty: ValueShape::Integer,
                },
                ValueDecl {
                    id: function_value,
                    ty: super::super::ValueShape::Reference(Reference {
                        nullable: false,
                        heap: super::super::RefShape::Closure(signature),
                    }),
                },
            ],
            assignments: vec![Assignment {
                destination: function_value,
                kind: AssignmentKind::FunctionRef {
                    function: symbol(1),
                    signature,
                    captures: vec![capture],
                },
                span: TextRange::new(0, 1),
            }],
            result: function_value,
            result_type: super::super::ValueShape::Reference(Reference {
                nullable: false,
                heap: super::super::RefShape::Closure(signature),
            }),
            span: TextRange::new(0, 1),
        };
        let mut representations = table();
        representations.signatures.push(super::super::Signature {
            parameters: Vec::new(),
            result: ValueShape::Integer,
        });
        let module = Module {
            name: "invalid".into(),
            externals: Vec::new(),
            representations,
            functions: vec![caller, target],
            entry: None,
            span: TextRange::new(0, 1),
        };

        assert!(verify_module(&module).is_err());
    }

    #[test]
    fn rejects_a_box_projection_with_the_wrong_result_shape() {
        let mut representations = table();
        let boxed = representations.reserve();
        representations.set(
            boxed,
            super::super::Representation::Box {
                value: ValueShape::Integer,
            },
        );
        let value = super::super::ValueId(0);
        let destination = super::super::ValueId(1);
        let function = Function {
            symbol: symbol(0),
            name: "invalid".into(),
            parameters: vec![value],
            values: vec![
                ValueDecl {
                    id: value,
                    ty: super::super::ValueShape::Reference(Reference {
                        nullable: false,
                        heap: super::super::RefShape::Repr(boxed),
                    }),
                },
                ValueDecl {
                    id: destination,
                    ty: ValueShape::Number,
                },
            ],
            assignments: vec![Assignment {
                destination,
                kind: AssignmentKind::ProductGet {
                    destination,
                    representation: boxed,
                    field: 0,
                    value,
                },
                span: TextRange::new(0, 1),
            }],
            result: destination,
            result_type: ValueShape::Number,
            span: TextRange::new(0, 1),
        };

        assert!(verify_function(&function, &HashMap::new(), &representations).is_err());
    }
}

use super::*;
use crate::cc::{
    Assignment, AssignmentKind, RefShape, Reference, Representation, ValueDecl, ValueShape,
};
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

fn binary_operation_function(
    op: super::super::BinaryOp,
    operand: ValueShape,
    result: ValueShape,
) -> Function {
    let left = super::super::ValueId(0);
    let right = super::super::ValueId(1);
    let destination = super::super::ValueId(2);
    Function {
        symbol: symbol(0),
        name: "binary_operation".into(),
        parameters: vec![left, right],
        values: vec![
            ValueDecl {
                id: left,
                ty: operand,
            },
            ValueDecl {
                id: right,
                ty: operand,
            },
            ValueDecl {
                id: destination,
                ty: result,
            },
        ],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::Primitive { op, left, right },
            span: TextRange::new(0, 1),
        }],
        result: destination,
        result_type: result,
        span: TextRange::new(0, 1),
    }
}

#[test]
fn rejects_integer_arithmetic_on_number_operands() {
    let function = binary_operation_function(
        super::super::BinaryOp::IntAdd,
        ValueShape::Number,
        ValueShape::Integer,
    );
    assert!(verify_function(&function, &HashMap::new(), &table()).is_err());
}

#[test]
fn rejects_number_comparisons_on_integer_operands() {
    let function = binary_operation_function(
        super::super::BinaryOp::NumberEq,
        ValueShape::Integer,
        ValueShape::Boolean,
    );
    assert!(verify_function(&function, &HashMap::new(), &table()).is_err());
}

#[test]
fn rejects_boolean_logic_on_integer_operands() {
    let function = binary_operation_function(
        super::super::BinaryOp::BooleanAnd,
        ValueShape::Integer,
        ValueShape::Boolean,
    );
    assert!(verify_function(&function, &HashMap::new(), &table()).is_err());
}

#[test]
fn rejects_a_comparison_with_an_integer_result() {
    let function = binary_operation_function(
        super::super::BinaryOp::IntLt,
        ValueShape::Integer,
        ValueShape::Integer,
    );
    assert!(verify_function(&function, &HashMap::new(), &table()).is_err());
}

#[test]
fn rejects_int_to_number_on_a_number_operand() {
    let input = super::super::ValueId(0);
    let destination = super::super::ValueId(1);
    let function = Function {
        symbol: symbol(0),
        name: "wrong_conversion_operand".into(),
        parameters: vec![input],
        values: vec![
            ValueDecl {
                id: input,
                ty: ValueShape::Number,
            },
            ValueDecl {
                id: destination,
                ty: ValueShape::Number,
            },
        ],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::Unary {
                op: super::super::UnaryOp::IntToNumber,
                value: input,
            },
            span: TextRange::new(0, 1),
        }],
        result: destination,
        result_type: ValueShape::Number,
        span: TextRange::new(0, 1),
    };
    assert!(verify_function(&function, &HashMap::new(), &table()).is_err());
}

#[test]
fn rejects_a_unary_operation_with_the_wrong_operand_shape() {
    let input = super::super::ValueId(0);
    let destination = super::super::ValueId(1);
    let function = Function {
        symbol: symbol(0),
        name: "invalid".into(),
        parameters: vec![input],
        values: vec![
            ValueDecl {
                id: input,
                ty: ValueShape::Integer,
            },
            ValueDecl {
                id: destination,
                ty: ValueShape::Number,
            },
        ],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::Unary {
                op: super::super::UnaryOp::NumberNeg,
                value: input,
            },
            span: TextRange::new(0, 1),
        }],
        result: destination,
        result_type: ValueShape::Number,
        span: TextRange::new(0, 1),
    };

    assert!(verify_function(&function, &HashMap::new(), &table()).is_err());
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

fn boxed_integer_table() -> (RepresentationTable, super::super::ReprId) {
    let mut representations = RepresentationTable::default();
    let boxed = representations.reserve();
    representations.set(
        boxed,
        Representation::Box {
            value: ValueShape::Integer,
        },
    );
    (representations, boxed)
}

fn adaptation_function(source: ValueShape, target: Reference) -> Function {
    let value = super::super::ValueId(0);
    let destination = super::super::ValueId(1);
    Function {
        symbol: symbol(0),
        name: "adapt".into(),
        parameters: vec![value],
        values: vec![
            ValueDecl {
                id: value,
                ty: source,
            },
            ValueDecl {
                id: destination,
                ty: ValueShape::Reference(target),
            },
        ],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::RepresentationCast {
                destination,
                value,
                reference: target,
            },
            span: TextRange::new(0, 1),
        }],
        result: destination,
        result_type: ValueShape::Reference(target),
        span: TextRange::new(0, 1),
    }
}

#[test]
fn rejects_a_nullable_erased_source_crossing_to_a_concrete_reference() {
    let (representations, boxed) = boxed_integer_table();
    let source = ValueShape::Reference(Reference {
        nullable: true,
        heap: RefShape::Erased,
    });
    let target = Reference {
        nullable: false,
        heap: RefShape::Repr(boxed),
    };
    let function = adaptation_function(source, target);

    let errors = verify_function(&function, &HashMap::new(), &representations)
        .expect_err("a nullable erased source is not the exact erased requirement");
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P8 CC verification")
    );
}

#[test]
fn rejects_a_nullable_erased_target_crossing_from_a_concrete_reference() {
    let (representations, boxed) = boxed_integer_table();
    let source = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(boxed),
    });
    let target = Reference {
        nullable: true,
        heap: RefShape::Erased,
    };
    let function = adaptation_function(source, target);

    let errors = verify_function(&function, &HashMap::new(), &representations)
        .expect_err("a nullable erased target is not the exact erased requirement");
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P8 CC verification")
    );
}

#[test]
fn accepts_exact_non_null_erased_boundary_casts() {
    let (representations, boxed) = boxed_integer_table();
    for (source, target) in [
        (
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            }),
            Reference {
                nullable: false,
                heap: RefShape::Repr(boxed),
            },
        ),
        (
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(boxed),
            }),
            Reference {
                nullable: false,
                heap: RefShape::Erased,
            },
        ),
    ] {
        let function = adaptation_function(source, target);
        assert!(
            verify_function(&function, &HashMap::new(), &representations).is_ok(),
            "exact non-null erased adaptation must remain valid"
        );
    }
}

#[test]
fn accepts_a_same_heap_nullability_change_for_loop_carried_references() {
    let (representations, _) = boxed_integer_table();
    let function = adaptation_function(
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        }),
        Reference {
            nullable: true,
            heap: RefShape::Erased,
        },
    );

    assert!(verify_function(&function, &HashMap::new(), &representations).is_ok());
}

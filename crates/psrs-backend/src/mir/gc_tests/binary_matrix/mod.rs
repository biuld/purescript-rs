use super::*;
use crate::cc::{BinaryOp, ValueDecl};

#[derive(Clone, Copy)]
enum Expected {
    Integer(i32),
    Number(&'static str),
    Boolean(bool),
}

#[test]
fn verifies_and_executes_every_cc_binary_scalar_variant_on_both_targets() {
    let symbol = SymbolId::new(ModuleId(0), 0);
    let mut values = Vec::new();
    let mut assignments = Vec::new();
    for (id, (ty, value)) in [
        (ValueShape::Integer, 7),
        (ValueShape::Integer, 2),
        (ValueShape::Number, 0),
        (ValueShape::Number, 0),
        (ValueShape::Boolean, 1),
        (ValueShape::Boolean, 0),
        (ValueShape::Integer, 65),
        (ValueShape::Integer, 66),
    ]
    .into_iter()
    .enumerate()
    {
        let destination = ValueId(id as u32);
        values.push(ValueDecl {
            id: destination,
            ty,
        });
        match ty {
            ValueShape::Number => {
                let number = if id == 2 { "7.5" } else { "2.0" };
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::NumberConstant(number.into()),
                    span: span(),
                });
            }
            ValueShape::Boolean | ValueShape::Integer => assignments.push(Assignment {
                destination,
                kind: AssignmentKind::Constant(value),
                span: span(),
            }),
            ValueShape::String => assignments.push(Assignment {
                destination,
                kind: AssignmentKind::StringConstant("text".into()),
                span: span(),
            }),
            ValueShape::Reference(_) => unreachable!(),
        }
    }

    let operations = [
        (
            BinaryOp::IntAdd,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(9),
        ),
        (
            BinaryOp::IntSub,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(5),
        ),
        (
            BinaryOp::IntMul,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(14),
        ),
        (
            BinaryOp::IntQuot,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(3),
        ),
        (
            BinaryOp::IntRem,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(1),
        ),
        (
            BinaryOp::IntDiv,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(3),
        ),
        (
            BinaryOp::IntMod,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(1),
        ),
        (
            BinaryOp::IntAnd,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(2),
        ),
        (
            BinaryOp::IntOr,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(7),
        ),
        (
            BinaryOp::IntXor,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(5),
        ),
        (
            BinaryOp::IntShl,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(28),
        ),
        (
            BinaryOp::IntShr,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(1),
        ),
        (
            BinaryOp::IntZshr,
            0,
            1,
            ValueShape::Integer,
            Expected::Integer(1),
        ),
        (
            BinaryOp::IntEq,
            0,
            1,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
        (
            BinaryOp::IntNe,
            0,
            1,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::IntLt,
            0,
            1,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
        (
            BinaryOp::IntLe,
            0,
            1,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
        (
            BinaryOp::IntGt,
            0,
            1,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::IntGe,
            0,
            1,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::NumberAdd,
            2,
            3,
            ValueShape::Number,
            Expected::Number("9.5"),
        ),
        (
            BinaryOp::NumberSub,
            2,
            3,
            ValueShape::Number,
            Expected::Number("5.5"),
        ),
        (
            BinaryOp::NumberMul,
            2,
            3,
            ValueShape::Number,
            Expected::Number("15.0"),
        ),
        (
            BinaryOp::NumberDiv,
            2,
            3,
            ValueShape::Number,
            Expected::Number("3.75"),
        ),
        (
            BinaryOp::NumberEq,
            2,
            3,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
        (
            BinaryOp::NumberNe,
            2,
            3,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::NumberLt,
            2,
            3,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
        (
            BinaryOp::NumberLe,
            2,
            3,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
        (
            BinaryOp::NumberGt,
            2,
            3,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::NumberGe,
            2,
            3,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::BooleanAnd,
            4,
            5,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
        (
            BinaryOp::BooleanOr,
            4,
            5,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::BooleanEq,
            4,
            5,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
        (
            BinaryOp::BooleanNe,
            4,
            5,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::CharEq,
            6,
            7,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
        (
            BinaryOp::CharNe,
            6,
            7,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::CharLt,
            6,
            7,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::CharLe,
            6,
            7,
            ValueShape::Boolean,
            Expected::Boolean(true),
        ),
        (
            BinaryOp::CharGt,
            6,
            7,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
        (
            BinaryOp::CharGe,
            6,
            7,
            ValueShape::Boolean,
            Expected::Boolean(false),
        ),
    ];
    let mut checks = Vec::new();
    let mut next = 8;
    for (op, left, right, result_shape, expected) in operations {
        let destination = ValueId(next);
        next += 1;
        values.push(ValueDecl {
            id: destination,
            ty: result_shape,
        });
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::Primitive {
                op,
                left: ValueId(left),
                right: ValueId(right),
            },
            span: span(),
        });
        let expected_value = ValueId(next);
        next += 1;
        let compare = match expected {
            Expected::Integer(value) => {
                values.push(ValueDecl {
                    id: expected_value,
                    ty: ValueShape::Integer,
                });
                assignments.push(Assignment {
                    destination: expected_value,
                    kind: AssignmentKind::Constant(value),
                    span: span(),
                });
                BinaryOp::IntEq
            }
            Expected::Number(value) => {
                values.push(ValueDecl {
                    id: expected_value,
                    ty: ValueShape::Number,
                });
                assignments.push(Assignment {
                    destination: expected_value,
                    kind: AssignmentKind::NumberConstant(value.into()),
                    span: span(),
                });
                BinaryOp::NumberEq
            }
            Expected::Boolean(value) => {
                values.push(ValueDecl {
                    id: expected_value,
                    ty: ValueShape::Boolean,
                });
                assignments.push(Assignment {
                    destination: expected_value,
                    kind: AssignmentKind::Constant(i32::from(value)),
                    span: span(),
                });
                BinaryOp::BooleanEq
            }
        };
        let check = ValueId(next);
        next += 1;
        values.push(ValueDecl {
            id: check,
            ty: ValueShape::Boolean,
        });
        assignments.push(Assignment {
            destination: check,
            kind: AssignmentKind::Primitive {
                op: compare,
                left: destination,
                right: expected_value,
            },
            span: span(),
        });
        checks.push(check);
    }

    let mut combined = checks[0];
    for check in checks.into_iter().skip(1) {
        let result = ValueId(next);
        next += 1;
        values.push(ValueDecl {
            id: result,
            ty: ValueShape::Boolean,
        });
        assignments.push(Assignment {
            destination: result,
            kind: AssignmentKind::Primitive {
                op: BinaryOp::BooleanAnd,
                left: combined,
                right: check,
            },
            span: span(),
        });
        combined = result;
    }
    let result = ValueId(next);
    let then_value = ValueId(next + 1);
    let else_value = ValueId(next + 2);
    values.extend([
        ValueDecl {
            id: result,
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: then_value,
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: else_value,
            ty: ValueShape::Integer,
        },
    ]);
    assignments.push(Assignment {
        destination: result,
        kind: AssignmentKind::If {
            condition: combined,
            then_assignments: vec![Assignment {
                destination: then_value,
                kind: AssignmentKind::Constant(0),
                span: span(),
            }],
            then_value,
            else_assignments: vec![Assignment {
                destination: else_value,
                kind: AssignmentKind::Constant(1),
                span: span(),
            }],
            else_value,
        },
        span: span(),
    });
    let module = CcModule {
        name: "BinaryScalarMatrix".into(),
        externals: Vec::new(),
        representations: RepresentationTable::default(),
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result,
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };
    let (gc_mir, _) = crate::mir::lower_module_with_capabilities(
        module.clone(),
        crate::TargetCapabilities::default(),
    )
    .expect("the complete binary scalar module should lower for GC");
    assert_eq!(gc_mir.functions.len(), 3);
    run_gc(&gc_mir, 0);
}

use super::*;

#[test]
fn lowers_unary_and_conversion_operations_on_both_targets() {
    use crate::cc::{UnaryOp, ValueDecl};

    let symbol = SymbolId::new(ModuleId(0), 0);
    let shapes = [
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Number,
        ValueShape::Integer,
        ValueShape::Number,
        ValueShape::Integer,
        ValueShape::Number,
        ValueShape::Integer,
        ValueShape::Boolean,
        ValueShape::Boolean,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Boolean,
        ValueShape::Integer,
        ValueShape::Boolean,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Number,
        ValueShape::Number,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
    ];
    let values = shapes
        .into_iter()
        .enumerate()
        .map(|(id, ty)| ValueDecl {
            id: ValueId(id as u32),
            ty,
        })
        .collect();
    let unary = |destination, op, value| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Unary {
            op,
            value: ValueId(value),
        },
        span: span(),
    };
    let constant = |destination, value| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Constant(value),
        span: span(),
    };
    let primitive = |destination, left, right| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Primitive {
            op: crate::cc::BinaryOp::IntAdd,
            left: ValueId(left),
            right: ValueId(right),
        },
        span: span(),
    };
    let module = CcModule {
        name: "UnaryScalars".into(),
        externals: Vec::new(),
        representations: RepresentationTable::default(),
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments: vec![
                constant(0, -5),
                unary(1, UnaryOp::IntNeg, 0),
                unary(2, UnaryOp::IntComplement, 0),
                Assignment {
                    destination: ValueId(3),
                    kind: AssignmentKind::NumberConstant("-2147483648.5".into()),
                    span: span(),
                },
                unary(4, UnaryOp::NumberToInt, 3),
                Assignment {
                    destination: ValueId(5),
                    kind: AssignmentKind::NumberConstant("2147483648".into()),
                    span: span(),
                },
                unary(6, UnaryOp::NumberToInt, 5),
                Assignment {
                    destination: ValueId(7),
                    kind: AssignmentKind::NumberConstant("NaN".into()),
                    span: span(),
                },
                unary(8, UnaryOp::NumberToInt, 7),
                constant(9, 1),
                unary(10, UnaryOp::BooleanNot, 9),
                unary(11, UnaryOp::BooleanToInt, 10),
                unary(12, UnaryOp::BooleanToInt, 9),
                constant(13, 0),
                unary(14, UnaryOp::IntToBoolean, 13),
                unary(15, UnaryOp::BooleanToInt, 14),
                unary(16, UnaryOp::IntToBoolean, 0),
                unary(17, UnaryOp::BooleanToInt, 16),
                constant(18, 65),
                unary(19, UnaryOp::CharToInt, 18),
                unary(20, UnaryOp::IntToChar, 19),
                unary(21, UnaryOp::IntToNumber, 18),
                unary(22, UnaryOp::NumberNeg, 21),
                unary(23, UnaryOp::NumberToInt, 21),
                unary(24, UnaryOp::NumberToInt, 22),
                primitive(25, 1, 2),
                primitive(26, 25, 4),
                primitive(27, 26, 6),
                primitive(28, 27, 8),
                primitive(29, 28, 11),
                primitive(30, 29, 12),
                primitive(31, 30, 15),
                primitive(32, 31, 17),
                primitive(33, 32, 19),
                primitive(34, 33, 20),
                primitive(35, 34, 23),
                primitive(36, 35, 24),
            ],
            result: ValueId(36),
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
    .expect("unary scalar CC should lower for the GC target");
    assert!(
        gc_mir.functions[0].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::UnaryPrimitive { .. }))
    );
    run_gc(&gc_mir, 140);
    run_linear(module, "140");
}

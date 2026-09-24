use super::*;

#[test]
fn lowers_typed_number_boolean_character_and_bitwise_operations_on_both_targets() {
    use crate::cc::BinaryOp;

    let symbol = SymbolId::new(ModuleId(0), 0);
    let value_types = [
        ValueShape::Number,
        ValueShape::Number,
        ValueShape::Number,
        ValueShape::Boolean,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Boolean,
        ValueShape::Boolean,
        ValueShape::Boolean,
        ValueShape::Boolean,
        ValueShape::Boolean,
        ValueShape::Boolean,
        ValueShape::Boolean,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
    ];
    let values = value_types
        .into_iter()
        .enumerate()
        .map(|(id, ty)| crate::cc::ValueDecl {
            id: ValueId(id as u32),
            ty,
        })
        .collect();
    let primitive = |destination, op, left, right| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Primitive {
            op,
            left: ValueId(left),
            right: ValueId(right),
        },
        span: span(),
    };
    let constant = |destination, value| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Constant(value),
        span: span(),
    };
    let module = CcModule {
        name: "TypedScalars".into(),
        externals: Vec::new(),
        representations: RepresentationTable::default(),
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments: vec![
                Assignment {
                    destination: ValueId(0),
                    kind: AssignmentKind::NumberConstant("3.5".into()),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::NumberConstant("2.0".into()),
                    span: span(),
                },
                primitive(2, BinaryOp::NumberAdd, 0, 1),
                primitive(3, BinaryOp::NumberGt, 0, 1),
                constant(4, 7),
                constant(5, 2),
                primitive(6, BinaryOp::IntXor, 4, 5),
                primitive(7, BinaryOp::IntShl, 4, 5),
                constant(8, 1),
                constant(9, 0),
                primitive(10, BinaryOp::BooleanAnd, 8, 9),
                primitive(11, BinaryOp::BooleanOr, 8, 9),
                primitive(12, BinaryOp::CharGt, 4, 5),
                primitive(13, BinaryOp::BooleanOr, 3, 11),
                primitive(14, BinaryOp::BooleanOr, 13, 12),
                primitive(15, BinaryOp::IntAdd, 6, 7),
                Assignment {
                    destination: ValueId(16),
                    kind: AssignmentKind::If {
                        condition: ValueId(14),
                        then_assignments: vec![constant(17, 33)],
                        then_value: ValueId(17),
                        else_assignments: vec![constant(18, -1)],
                        else_value: ValueId(18),
                    },
                    span: span(),
                },
            ],
            result: ValueId(16),
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
    .expect("typed scalar CC should lower for the GC target");
    let instructions = gc_mir.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            Instruction::Primitive { op, .. } => Some(*op),
            _ => None,
        })
        .collect::<Vec<_>>();
    for expected in [
        crate::mir::NumericOp::F64Add,
        crate::mir::NumericOp::F64Gt,
        crate::mir::NumericOp::I32Xor,
        crate::mir::NumericOp::I32Shl,
        crate::mir::NumericOp::BoolAnd,
        crate::mir::NumericOp::BoolOr,
        crate::mir::NumericOp::I32GtS,
    ] {
        assert!(instructions.contains(&expected), "missing {expected:?}");
    }
    run_gc(&gc_mir, 33);
    run_linear(module, "33");
}

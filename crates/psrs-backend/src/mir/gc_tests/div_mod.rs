use super::*;

#[test]
fn lowers_euclidean_integer_division_and_modulo_on_both_targets() {
    use crate::cc::BinaryOp;

    let symbol = SymbolId::new(ModuleId(0), 0);
    let values = (0..42)
        .map(|id| crate::cc::ValueDecl {
            id: ValueId(id),
            ty: if (24..=38).contains(&id) {
                ValueShape::Boolean
            } else {
                ValueShape::Integer
            },
        })
        .collect();
    let constant = |destination, value| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Constant(value),
        span: span(),
    };
    let binary = |destination, op, left, right| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Primitive {
            op,
            left: ValueId(left),
            right: ValueId(right),
        },
        span: span(),
    };
    let module = CcModule {
        name: "EuclideanScalars".into(),
        externals: Vec::new(),
        representations: RepresentationTable::default(),
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments: vec![
                constant(0, -5),
                constant(1, 3),
                binary(2, BinaryOp::IntDiv, 0, 1),
                binary(3, BinaryOp::IntMod, 0, 1),
                constant(4, 5),
                constant(5, -3),
                binary(6, BinaryOp::IntDiv, 4, 5),
                binary(7, BinaryOp::IntMod, 4, 5),
                constant(8, -5),
                constant(9, -3),
                binary(10, BinaryOp::IntDiv, 8, 9),
                binary(11, BinaryOp::IntMod, 8, 9),
                constant(12, 6),
                constant(13, -3),
                binary(14, BinaryOp::IntDiv, 12, 13),
                binary(15, BinaryOp::IntMod, 12, 13),
                constant(16, -2),
                constant(17, 1),
                constant(18, -2),
                constant(19, -1),
                constant(20, 1),
                constant(21, -2),
                constant(22, -2),
                constant(23, 0),
                binary(24, BinaryOp::IntEq, 2, 16),
                binary(25, BinaryOp::IntEq, 3, 17),
                binary(26, BinaryOp::IntEq, 6, 18),
                binary(27, BinaryOp::IntEq, 7, 19),
                binary(28, BinaryOp::IntEq, 10, 20),
                binary(29, BinaryOp::IntEq, 11, 21),
                binary(30, BinaryOp::IntEq, 14, 22),
                binary(31, BinaryOp::IntEq, 15, 23),
                binary(32, BinaryOp::BooleanAnd, 24, 25),
                binary(33, BinaryOp::BooleanAnd, 32, 26),
                binary(34, BinaryOp::BooleanAnd, 33, 27),
                binary(35, BinaryOp::BooleanAnd, 34, 28),
                binary(36, BinaryOp::BooleanAnd, 35, 29),
                binary(37, BinaryOp::BooleanAnd, 36, 30),
                binary(38, BinaryOp::BooleanAnd, 37, 31),
                Assignment {
                    destination: ValueId(39),
                    kind: AssignmentKind::If {
                        condition: ValueId(38),
                        then_assignments: vec![constant(40, 3)],
                        then_value: ValueId(40),
                        else_assignments: vec![constant(41, -1)],
                        else_value: ValueId(41),
                    },
                    span: span(),
                },
            ],
            result: ValueId(39),
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
    .expect("Euclidean scalar CC should lower for the GC target");
    assert_eq!(gc_mir.functions.len(), 3);
    assert_eq!(gc_mir.functions[1].name, "__psrs_euclidean_int_div");
    assert_eq!(gc_mir.functions[2].name, "__psrs_euclidean_int_mod");
    assert!(
        gc_mir.functions[0]
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| matches!(instruction, Instruction::Call { .. }))
            .count()
            >= 8
    );
    run_gc(&gc_mir, 3);
}

#[test]
fn generates_helpers_for_division_and_modulo_inside_tag_switch_arms() {
    use crate::cc::{BinaryOp, TagCase};

    let symbol = SymbolId::new(ModuleId(0), 0);
    let values = (0..=8)
        .map(|id| crate::cc::ValueDecl {
            id: ValueId(id),
            ty: ValueShape::Integer,
        })
        .collect();
    let constant = |destination, value| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Constant(value),
        span: span(),
    };
    let binary = |destination, op, left, right| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Primitive {
            op,
            left: ValueId(left),
            right: ValueId(right),
        },
        span: span(),
    };
    let module = CcModule {
        name: "TagSwitchScalars".into(),
        externals: Vec::new(),
        representations: RepresentationTable::default(),
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments: vec![
                constant(0, 0),
                Assignment {
                    destination: ValueId(5),
                    kind: AssignmentKind::TagSwitch {
                        value: ValueId(0),
                        cases: vec![TagCase {
                            tag: 0,
                            assignments: vec![
                                constant(2, 7),
                                constant(3, 3),
                                binary(4, BinaryOp::IntDiv, 2, 3),
                            ],
                            value: ValueId(4),
                        }],
                        default_assignments: vec![
                            constant(6, 7),
                            constant(7, 3),
                            binary(8, BinaryOp::IntMod, 6, 7),
                        ],
                        default_value: ValueId(8),
                    },
                    span: span(),
                },
            ],
            result: ValueId(5),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };

    let (gc_mir, _) =
        crate::mir::lower_module_with_capabilities(module, crate::TargetCapabilities::default())
            .expect("division and modulo inside a tag switch should lower");
    assert!(
        gc_mir
            .functions
            .iter()
            .any(|function| function.name == "__psrs_euclidean_int_div"),
        "the IntDiv helper must be generated for a case arm"
    );
    assert!(
        gc_mir
            .functions
            .iter()
            .any(|function| function.name == "__psrs_euclidean_int_mod"),
        "the IntMod helper must be generated for the default arm"
    );
    run_gc(&gc_mir, 2);
}

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
fn detects_division_and_modulo_nested_in_tag_switch_cases() {
    use crate::cc::{BinaryOp, TagCase};

    let symbol = SymbolId::new(ModuleId(0), 0);
    let values = (0..=7)
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
        name: "NestedEuclideanScalars".into(),
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
                    destination: ValueId(7),
                    kind: AssignmentKind::TagSwitch {
                        value: ValueId(0),
                        cases: vec![TagCase {
                            tag: 0,
                            assignments: vec![
                                constant(1, 7),
                                constant(2, 2),
                                binary(3, BinaryOp::IntDiv, 1, 2),
                                binary(4, BinaryOp::IntMod, 1, 2),
                                binary(5, BinaryOp::IntAdd, 3, 4),
                            ],
                            value: ValueId(5),
                        }],
                        default_assignments: vec![constant(6, 0)],
                        default_value: ValueId(6),
                    },
                    span: span(),
                },
            ],
            result: ValueId(7),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };

    let (gc_mir, _) =
        crate::mir::lower_module_with_capabilities(module, crate::TargetCapabilities::default())
            .expect("division nested in a tag switch must still generate its helper");
    assert_eq!(gc_mir.functions.len(), 3);
    assert!(
        gc_mir
            .functions
            .iter()
            .any(|function| function.name == "__psrs_euclidean_int_div"),
        "the nested divide must allocate the div helper"
    );
    assert!(
        gc_mir
            .functions
            .iter()
            .any(|function| function.name == "__psrs_euclidean_int_mod"),
        "the nested modulo must allocate the mod helper"
    );
    run_gc(&gc_mir, 4);
}

#[test]
fn detects_modulo_nested_in_a_tag_switch_default_arm() {
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
        name: "DefaultEuclideanScalars".into(),
        externals: Vec::new(),
        representations: RepresentationTable::default(),
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments: vec![
                constant(0, 1),
                Assignment {
                    destination: ValueId(8),
                    kind: AssignmentKind::TagSwitch {
                        value: ValueId(0),
                        cases: vec![TagCase {
                            tag: 0,
                            assignments: vec![
                                constant(1, 7),
                                constant(2, 2),
                                binary(3, BinaryOp::IntDiv, 1, 2),
                            ],
                            value: ValueId(3),
                        }],
                        default_assignments: vec![
                            constant(4, 7),
                            constant(5, 2),
                            binary(6, BinaryOp::IntMod, 4, 5),
                        ],
                        default_value: ValueId(6),
                    },
                    span: span(),
                },
            ],
            result: ValueId(8),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };

    let (gc_mir, _) =
        crate::mir::lower_module_with_capabilities(module, crate::TargetCapabilities::default())
            .expect("modulo in a tag switch default arm must still generate its helper");
    assert_eq!(gc_mir.functions.len(), 3);
    assert!(
        gc_mir
            .functions
            .iter()
            .any(|function| function.name == "__psrs_euclidean_int_div")
    );
    assert!(
        gc_mir
            .functions
            .iter()
            .any(|function| function.name == "__psrs_euclidean_int_mod")
    );
    run_gc(&gc_mir, 1);
}

#[test]
fn does_not_emit_helpers_without_division_or_modulo() {
    use crate::cc::BinaryOp;

    let symbol = SymbolId::new(ModuleId(0), 0);
    let values = (0..=3)
        .map(|id| crate::cc::ValueDecl {
            id: ValueId(id),
            ty: ValueShape::Integer,
        })
        .collect();
    let module = CcModule {
        name: "NoEuclideanScalars".into(),
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
                    kind: AssignmentKind::Constant(7),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::Constant(2),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::Primitive {
                        op: BinaryOp::IntAdd,
                        left: ValueId(0),
                        right: ValueId(1),
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(3),
                    kind: AssignmentKind::Primitive {
                        op: BinaryOp::IntQuot,
                        left: ValueId(0),
                        right: ValueId(1),
                    },
                    span: span(),
                },
            ],
            result: ValueId(2),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };

    let (gc_mir, _) =
        crate::mir::lower_module_with_capabilities(module, crate::TargetCapabilities::default())
            .expect("truncated quotient must not require a floor helper");
    assert_eq!(gc_mir.functions.len(), 1);
    run_gc(&gc_mir, 9);
}

#[test]
fn skips_helper_symbols_used_by_module_functions() {
    use crate::cc::BinaryOp;

    let colliding = SymbolId::new(ModuleId(0), u32::MAX);
    let values = (0..=4)
        .map(|id| crate::cc::ValueDecl {
            id: ValueId(id),
            ty: ValueShape::Integer,
        })
        .collect();
    let module = CcModule {
        name: "CollidingScalarSymbols".into(),
        externals: Vec::new(),
        representations: RepresentationTable::default(),
        functions: vec![CcFunction {
            symbol: colliding,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments: vec![
                Assignment {
                    destination: ValueId(0),
                    kind: AssignmentKind::Constant(7),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::Constant(2),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::Primitive {
                        op: BinaryOp::IntDiv,
                        left: ValueId(0),
                        right: ValueId(1),
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(3),
                    kind: AssignmentKind::Primitive {
                        op: BinaryOp::IntMod,
                        left: ValueId(0),
                        right: ValueId(1),
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(4),
                    kind: AssignmentKind::Primitive {
                        op: BinaryOp::IntAdd,
                        left: ValueId(2),
                        right: ValueId(3),
                    },
                    span: span(),
                },
            ],
            result: ValueId(4),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(colliding),
        span: span(),
    };

    let (gc_mir, _) =
        crate::mir::lower_module_with_capabilities(module, crate::TargetCapabilities::default())
            .expect("the helper allocator must skip the colliding module symbol");
    assert_eq!(gc_mir.functions.len(), 3);
    let helpers = gc_mir
        .functions
        .iter()
        .filter(|function| function.name.starts_with("__psrs_euclidean"))
        .collect::<Vec<_>>();
    assert_eq!(helpers.len(), 2);
    let mut symbols = std::collections::HashSet::new();
    for helper in helpers {
        assert_ne!(
            helper.symbol, colliding,
            "a generated helper reused a module symbol"
        );
        assert!(
            symbols.insert(helper.symbol),
            "generated helpers share a symbol"
        );
    }
    run_gc(&gc_mir, 4);
}

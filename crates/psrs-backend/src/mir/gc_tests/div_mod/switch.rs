use super::*;

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

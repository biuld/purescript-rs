use super::*;
use crate::cc::{RefShape, Reference, Representation, ValueDecl};

#[test]
fn executes_erased_identity_for_scalars_and_concrete_references_on_both_targets() {
    let mut representations = RepresentationTable::default();
    let integer_box = representations.reserve();
    representations.set(
        integer_box,
        Representation::Box {
            value: ValueShape::Integer,
        },
    );
    let number_box = representations.reserve();
    representations.set(
        number_box,
        Representation::Box {
            value: ValueShape::Number,
        },
    );
    let product = representations.reserve();
    representations.set(
        product,
        Representation::Product {
            fields: vec![ValueShape::Integer],
        },
    );

    let symbol = SymbolId::new(ModuleId(0), 0);
    let identity_symbol = SymbolId::new(ModuleId(0), 1);
    let erased = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    });
    let integer_box_ref = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(integer_box),
    });
    let number_box_ref = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(number_box),
    });
    let product_ref = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(product),
    });
    let value_types = [
        ValueShape::Integer,
        integer_box_ref,
        erased,
        erased,
        integer_box_ref,
        ValueShape::Integer,
        ValueShape::Number,
        number_box_ref,
        erased,
        erased,
        number_box_ref,
        ValueShape::Number,
        ValueShape::Integer,
        ValueShape::Integer,
        product_ref,
        erased,
        erased,
        product_ref,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
    ];
    let values = value_types
        .into_iter()
        .enumerate()
        .map(|(id, ty)| ValueDecl {
            id: ValueId(id as u32),
            ty,
        })
        .collect();
    let cast = |destination, value, reference| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::RepresentationCast {
            destination: ValueId(destination),
            value: ValueId(value),
            reference,
        },
        span: span(),
    };
    let call_identity = |destination, value| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::DirectCall {
            function: identity_symbol,
            arguments: vec![ValueId(value)],
        },
        span: span(),
    };
    let module = CcModule {
        name: "ErasedIdentity".into(),
        externals: Vec::new(),
        representations,
        functions: vec![
            CcFunction {
                symbol,
                name: "main".into(),
                parameters: Vec::new(),
                values,
                assignments: vec![
                    Assignment {
                        destination: ValueId(0),
                        kind: AssignmentKind::Constant(23),
                        span: span(),
                    },
                    Assignment {
                        destination: ValueId(1),
                        kind: AssignmentKind::ProductNew {
                            destination: ValueId(1),
                            representation: integer_box,
                            arguments: vec![ValueId(0)],
                        },
                        span: span(),
                    },
                    cast(
                        2,
                        1,
                        Reference {
                            nullable: false,
                            heap: RefShape::Erased,
                        },
                    ),
                    call_identity(3, 2),
                    cast(
                        4,
                        3,
                        Reference {
                            nullable: false,
                            heap: RefShape::Repr(integer_box),
                        },
                    ),
                    Assignment {
                        destination: ValueId(5),
                        kind: AssignmentKind::ProductGet {
                            destination: ValueId(5),
                            representation: integer_box,
                            field: 0,
                            value: ValueId(4),
                        },
                        span: span(),
                    },
                    Assignment {
                        destination: ValueId(6),
                        kind: AssignmentKind::NumberConstant("42.0".into()),
                        span: span(),
                    },
                    Assignment {
                        destination: ValueId(7),
                        kind: AssignmentKind::ProductNew {
                            destination: ValueId(7),
                            representation: number_box,
                            arguments: vec![ValueId(6)],
                        },
                        span: span(),
                    },
                    cast(
                        8,
                        7,
                        Reference {
                            nullable: false,
                            heap: RefShape::Erased,
                        },
                    ),
                    call_identity(9, 8),
                    cast(
                        10,
                        9,
                        Reference {
                            nullable: false,
                            heap: RefShape::Repr(number_box),
                        },
                    ),
                    Assignment {
                        destination: ValueId(11),
                        kind: AssignmentKind::ProductGet {
                            destination: ValueId(11),
                            representation: number_box,
                            field: 0,
                            value: ValueId(10),
                        },
                        span: span(),
                    },
                    Assignment {
                        destination: ValueId(12),
                        kind: AssignmentKind::Unary {
                            op: crate::cc::UnaryOp::NumberToInt,
                            value: ValueId(11),
                        },
                        span: span(),
                    },
                    Assignment {
                        destination: ValueId(13),
                        kind: AssignmentKind::Constant(5),
                        span: span(),
                    },
                    Assignment {
                        destination: ValueId(14),
                        kind: AssignmentKind::ProductNew {
                            destination: ValueId(14),
                            representation: product,
                            arguments: vec![ValueId(13)],
                        },
                        span: span(),
                    },
                    cast(
                        15,
                        14,
                        Reference {
                            nullable: false,
                            heap: RefShape::Erased,
                        },
                    ),
                    call_identity(16, 15),
                    cast(
                        17,
                        16,
                        Reference {
                            nullable: false,
                            heap: RefShape::Repr(product),
                        },
                    ),
                    Assignment {
                        destination: ValueId(18),
                        kind: AssignmentKind::ProductGet {
                            destination: ValueId(18),
                            representation: product,
                            field: 0,
                            value: ValueId(17),
                        },
                        span: span(),
                    },
                    Assignment {
                        destination: ValueId(19),
                        kind: AssignmentKind::Primitive {
                            op: crate::cc::BinaryOp::IntAdd,
                            left: ValueId(5),
                            right: ValueId(12),
                        },
                        span: span(),
                    },
                    Assignment {
                        destination: ValueId(20),
                        kind: AssignmentKind::Primitive {
                            op: crate::cc::BinaryOp::IntAdd,
                            left: ValueId(19),
                            right: ValueId(18),
                        },
                        span: span(),
                    },
                ],
                result: ValueId(20),
                result_type: ValueShape::Integer,
                span: span(),
            },
            CcFunction {
                symbol: identity_symbol,
                name: "identity".into(),
                parameters: vec![ValueId(0)],
                values: vec![ValueDecl {
                    id: ValueId(0),
                    ty: erased,
                }],
                assignments: Vec::new(),
                result: ValueId(0),
                result_type: erased,
                span: span(),
            },
        ],
        entry: Some(symbol),
        span: span(),
    };

    let (gc_mir, _) = crate::mir::lower_module(module.clone())
        .expect("erased identity should lower through the GC planner");
    run_gc(&gc_mir, 70);
}

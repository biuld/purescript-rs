use super::*;
use crate::cc::{RefShape, Reference};
use psrs_core::{
    Binder, ConstructorInfo, Declaration, Expr, ExprKind, Module, Type, TypeConstructor,
};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};

#[test]
fn parameter_dependent_record_field_keeps_canonical_array_and_erases_the_adt_slot() {
    let module_id = ModuleId(0);
    let wrap_type = HirTypeId::new(module_id, 0);
    let wrap = SymbolId::new(module_id, 0);
    let array_a = TypeId(3);
    let record_a = TypeId(4);
    let module = Module {
        id: module_id,
        name: "RecordPayloadLayoutTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::User(wrap_type)),
            Type::Variable(TypeVariableId(0)),
            Type::Constructor(TypeConstructor::Array),
            Type::Application(TypeId(2), TypeId(1)),
            Type::Record(vec![("values".into(), array_a)]),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: vec![ConstructorInfo {
            symbol: wrap,
            name: "Wrap".into(),
            type_id: wrap_type,
            tag: 0,
            field_count: 1,
            field_types: vec![record_a],
        }],
        declarations: Vec::new(),
        entry: None,
        span: psrs_span::TextRange::new(0, 40),
    };
    let newtypes = HashSet::new();
    let enums = enum_type_ids(&module, &newtypes);
    let aggregates = aggregate_type_ids(&module, &newtypes);
    assert!(aggregates.contains(&wrap_type));
    let layout = type_layout(&module, &enums, &aggregates, &newtypes)
        .expect("parameterized record field layout should be supported");
    let array_repr = layout.array_types[&array_a];
    let record_repr = layout.record_types[&record_a];
    let erased_shape = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    });
    let canonical_array_shape = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(array_repr),
    });
    assert_eq!(
        layout.representations.representation(array_repr),
        Some(&Representation::Array {
            element: erased_shape,
        })
    );
    assert_eq!(
        layout.representations.representation(record_repr),
        Some(&Representation::Product {
            fields: vec![canonical_array_shape],
        })
    );
    let representation = layout.constructor_types[&wrap];
    assert_eq!(
        layout.representations.representation(representation),
        Some(&Representation::Variant {
            cases: vec![VariantCase {
                tag: 0,
                fields: vec![erased_shape],
            }],
        })
    );
}

fn empty_module(types: Vec<Type>) -> Module {
    Module {
        id: ModuleId(0),
        name: "LayoutKeysTest".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: psrs_span::TextRange::new(0, 1),
    }
}

fn layout_for(module: &Module) -> TypeLayout {
    let newtypes = HashSet::new();
    let enums = enum_type_ids(module, &newtypes);
    let aggregates = aggregate_type_ids(module, &newtypes);
    type_layout(module, &enums, &aggregates, &newtypes).expect("layout should succeed")
}

#[test]
fn canonical_record_keys_sort_labels_and_share_equal_keyed_records() {
    let module = empty_module(vec![
        Type::Variable(TypeVariableId(0)),
        Type::I32,
        Type::Record(vec![("x".into(), TypeId(0)), ("y".into(), TypeId(1))]),
        Type::Record(vec![("y".into(), TypeId(1)), ("x".into(), TypeId(0))]),
    ]);
    let layout = layout_for(&module);
    let first = layout.record_types[&TypeId(2)];
    let second = layout.record_types[&TypeId(3)];
    assert_eq!(
        first, second,
        "records with the same canonical field set must share a handle"
    );
    assert_eq!(
        layout.representations.product_labels(first),
        Some(["x".to_owned(), "y".to_owned()].as_slice())
    );
    assert_eq!(
        layout.representations.representation(first),
        Some(&Representation::Product {
            fields: vec![
                ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Erased,
                }),
                ValueShape::Integer,
            ],
        })
    );
}

#[test]
fn canonical_arrays_key_by_element_shape() {
    let module = empty_module(vec![
        Type::Variable(TypeVariableId(0)),
        Type::Constructor(TypeConstructor::Array),
        Type::Application(TypeId(1), TypeId(0)),
        Type::I32,
        Type::Application(TypeId(1), TypeId(3)),
        Type::Application(TypeId(1), TypeId(2)),
    ]);
    let layout = layout_for(&module);
    let generic = layout.array_types[&TypeId(2)];
    let concrete = layout.array_types[&TypeId(4)];
    let nested = layout.array_types[&TypeId(5)];
    assert_ne!(generic, concrete, "Array a and Array Int must differ");
    assert_eq!(
        layout.representations.representation(generic),
        Some(&Representation::Array {
            element: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            }),
        })
    );
    assert_eq!(
        layout.representations.representation(concrete),
        Some(&Representation::Array {
            element: ValueShape::Integer,
        })
    );
    assert_eq!(
        layout.representations.representation(nested),
        Some(&Representation::Array {
            element: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(generic),
            }),
        })
    );
}

#[test]
fn recursive_aggregate_normalization_terminates() {
    let module = empty_module(vec![
        Type::Application(TypeId(2), TypeId(1)),
        Type::Application(TypeId(2), TypeId(0)),
        Type::Constructor(TypeConstructor::Array),
    ]);
    let layout = layout_for(&module);
    let first = layout.array_types[&TypeId(0)];
    let second = layout.array_types[&TypeId(1)];
    assert!(matches!(
        layout.representations.representation(first),
        Some(Representation::Array { .. })
    ));
    assert!(matches!(
        layout.representations.representation(second),
        Some(Representation::Array { .. })
    ));
    assert_ne!(
        first, second,
        "mutually recursive arrays keep distinct canonical handles"
    );
}

#[test]
fn equal_normalized_function_signatures_share_one_signature_id() {
    let array = TypeId(0);
    let int = TypeId(1);
    let string = TypeId(2);
    let array_int = TypeId(3);
    let array_string = TypeId(4);
    let array_int_b = TypeId(5);
    let f_int = TypeId(6);
    let f_string = TypeId(7);
    let f_int_b = TypeId(8);
    let lambda = |parameter: TypeId, symbol: SymbolId, name: &str| Declaration {
        symbol,
        name: name.into(),
        name_span: psrs_span::TextRange::new(0, 1),
        quantified: Vec::new(),
        ty: parameter,
        value: Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "values".into(),
                    ty: parameter,
                    span: psrs_span::TextRange::new(0, 1),
                },
                body: Box::new(Expr {
                    kind: ExprKind::Local(LocalId(0)),
                    ty: parameter,
                    span: psrs_span::TextRange::new(0, 1),
                }),
            },
            ty: parameter,
            span: psrs_span::TextRange::new(0, 1),
        },
        span: psrs_span::TextRange::new(0, 1),
    };
    let module = Module {
        id: ModuleId(0),
        name: "SignatureInterningTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::Array),
            Type::I32,
            Type::String,
            Type::Application(array, int),
            Type::Application(array, string),
            Type::Application(array, int),
            Type::Function {
                parameter: array_int,
                result: array_int,
            },
            Type::Function {
                parameter: array_string,
                result: array_string,
            },
            Type::Function {
                parameter: array_int_b,
                result: array_int_b,
            },
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            lambda(f_int, SymbolId::new(ModuleId(0), 0), "fInt"),
            lambda(f_string, SymbolId::new(ModuleId(0), 1), "fStr"),
            lambda(f_int_b, SymbolId::new(ModuleId(0), 2), "fIntB"),
        ],
        entry: None,
        span: psrs_span::TextRange::new(0, 40),
    };
    let newtypes = HashSet::new();
    let enums = enum_type_ids(&module, &newtypes);
    let aggregates = aggregate_type_ids(&module, &newtypes);
    let layout = type_layout(&module, &enums, &aggregates, &newtypes)
        .expect("distinct function types should normalize and intern");

    assert_ne!(
        layout.array_types[&array_int], layout.array_types[&array_string],
        "Array Int and Array String are distinct semantic shapes with distinct canonical arrays"
    );
    assert_eq!(
        layout.array_types[&array_int], layout.array_types[&array_int_b],
        "equal array element shapes share one canonical array"
    );
    let first = layout.function_types[&f_int];
    let second = layout.function_types[&f_int_b];
    assert_eq!(
        first, second,
        "function types that are equal after normalization must share one SignatureId"
    );
    assert_ne!(
        layout.function_types[&f_int], layout.function_types[&f_string],
        "arrays with different semantic element shapes keep distinct signatures"
    );
    let signature = layout
        .representations
        .signature(first)
        .expect("the shared signature is present in the table");
    assert_eq!(signature.parameters.len(), 1);
    assert_eq!(signature.result, signature.parameters[0]);
    assert_eq!(
        signature.parameters[0],
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(layout.array_types[&array_int]),
        })
    );
}

fn integer_capture_module(capture: Type) -> Module {
    let symbol = SymbolId::new(ModuleId(0), 0);
    Module {
        id: ModuleId(0),
        name: "IntegerCaptureTest".into(),
        externals: Vec::new(),
        types: vec![capture, Type::I32],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol,
            name: "captures".into(),
            name_span: psrs_span::TextRange::new(0, 1),
            quantified: Vec::new(),
            ty: TypeId(1),
            value: Expr {
                kind: ExprKind::Lambda {
                    binder: Binder {
                        id: LocalId(1),
                        name: "argument".into(),
                        ty: TypeId(1),
                        span: psrs_span::TextRange::new(0, 1),
                    },
                    body: Box::new(Expr {
                        kind: ExprKind::Local(LocalId(0)),
                        ty: TypeId(0),
                        span: psrs_span::TextRange::new(0, 1),
                    }),
                },
                ty: TypeId(1),
                span: psrs_span::TextRange::new(0, 1),
            },
            span: psrs_span::TextRange::new(0, 1),
        }],
        entry: None,
        span: psrs_span::TextRange::new(0, 40),
    }
}

#[test]
fn non_i32_integer_shaped_captures_reserve_the_integer_box() {
    for capture in [Type::Char, Type::Unit] {
        let module = integer_capture_module(capture.clone());
        assert!(
            !module
                .types
                .iter()
                .any(|ty| matches!(ty, Type::Variable(_))),
            "the fixture must not contain a type variable"
        );
        let newtypes = HashSet::new();
        let enums = enum_type_ids(&module, &newtypes);
        let aggregates = aggregate_type_ids(&module, &newtypes);
        let layout = type_layout(&module, &enums, &aggregates, &newtypes)
            .expect("an integer-shaped capture should have a layout");
        assert!(
            layout.boxed_integer_type.is_some(),
            "a free {capture:?} capture maps to ValueShape::Integer and needs the integer box"
        );
    }
    // A `String` capture is a GC reference erased through the `eq` reference,
    // not through the one-field integer box.
    let module = integer_capture_module(Type::String);
    let newtypes = HashSet::new();
    let enums = enum_type_ids(&module, &newtypes);
    let aggregates = aggregate_type_ids(&module, &newtypes);
    let layout = type_layout(&module, &enums, &aggregates, &newtypes)
        .expect("a String capture should have a layout");
    assert!(
        layout.boxed_integer_type.is_none(),
        "a String capture must not reserve the integer box"
    );
}

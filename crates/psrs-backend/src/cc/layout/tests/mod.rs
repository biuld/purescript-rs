use super::*;
use crate::cc::{RefShape, Reference};
use psrs_core::{
    Binder, ConstructorInfo, Declaration, Expr, ExprKind, Module, Type, TypeConstructor,
};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};

mod records;

fn push_arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let head = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Function));
    let inner = TypeId(types.len() as u32);
    types.push(Type::Application(head, parameter));
    let outer = TypeId(types.len() as u32);
    types.push(Type::Application(inner, result));
    outer
}

fn empty_module(types: Vec<Type>) -> Module {
    Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "LayoutKeysTest".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
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
fn canonical_arrays_key_by_element_shape() {
    let module = empty_module(vec![
        Type::Variable(TypeVariableId(0)),
        Type::Constructor(TypeConstructor::Array),
        Type::Application(TypeId(1), TypeId(0)),
        Type::Constructor(psrs_core::TypeConstructor::Int),
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
    let mut types = vec![
        Type::Constructor(TypeConstructor::Array),
        Type::Constructor(psrs_core::TypeConstructor::Int),
        Type::Constructor(psrs_core::TypeConstructor::String),
        Type::Application(array, int),
        Type::Application(array, string),
        Type::Application(array, int),
    ];
    let f_int = push_arrow(&mut types, array_int, array_int);
    let f_string = push_arrow(&mut types, array_string, array_string);
    let f_int_b = push_arrow(&mut types, array_int_b, array_int_b);
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
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "SignatureInterningTest".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
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
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "IntegerCaptureTest".into(),
        externals: Vec::new(),
        types: vec![capture, Type::Constructor(psrs_core::TypeConstructor::Int)],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
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
    for capture in [
        Type::Constructor(psrs_core::TypeConstructor::Char),
        Type::Constructor(psrs_core::TypeConstructor::Unit),
    ] {
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
    let module = integer_capture_module(Type::Constructor(psrs_core::TypeConstructor::String));
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

#[test]
fn an_opaque_handle_and_an_array_of_handles_have_scalar_layouts() {
    let module_id = ModuleId(0);
    let opaque = HirTypeId::new(module_id, 0);
    let handle = TypeId(0);
    let array_handle = TypeId(2);
    let module = Module {
        type_names: Vec::new(),
        id: module_id,
        name: "OpaqueHandleLayoutTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::User(opaque)),
            Type::Constructor(TypeConstructor::Array),
            Type::Application(TypeId(1), TypeId(0)),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: vec![opaque],
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: psrs_span::TextRange::new(0, 40),
    };
    let newtypes = HashSet::new();
    let enums = enum_type_ids(&module, &newtypes);
    let aggregates = aggregate_type_ids(&module, &newtypes);
    let layout = type_layout(&module, &enums, &aggregates, &newtypes)
        .expect("an array of opaque handles should have a layout");
    assert!(
        layout.array_types.contains_key(&array_handle),
        "array<opaque> must reserve a canonical array layout"
    );
    assert_eq!(
        layout
            .representations
            .representation(layout.array_types[&array_handle]),
        Some(&Representation::Array {
            element: ValueShape::Integer,
        })
    );
    let _ = handle;
}

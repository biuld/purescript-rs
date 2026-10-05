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
        external_types: Vec::new(),
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

/// Roots aggregate types through declarations so the layout builder treats
/// them as live. The production builder only lays out types reachable from a
/// declaration or a constructor field, so a fixture with neither has no
/// arrays or records to normalize.
fn root_types(module: &mut Module, roots: impl IntoIterator<Item = TypeId>) {
    for (index, ty) in roots.into_iter().enumerate() {
        module.declarations.push(Declaration {
            symbol: SymbolId::new(module.id, index as u32),
            name: format!("root{index}"),
            name_span: psrs_span::TextRange::new(0, 1),
            quantified: Vec::new(),
            ty,
            value: Expr {
                kind: ExprKind::Unit,
                ty,
                span: psrs_span::TextRange::new(0, 1),
            },
            span: psrs_span::TextRange::new(0, 1),
        });
    }
}

#[test]
fn canonical_arrays_key_by_element_shape() {
    let mut module = empty_module(vec![
        Type::Variable(TypeVariableId(0)),
        Type::Constructor(TypeConstructor::Array),
        Type::Application(TypeId(1), TypeId(0)),
        Type::Constructor(psrs_core::TypeConstructor::Int),
        Type::Application(TypeId(1), TypeId(3)),
        Type::Application(TypeId(1), TypeId(2)),
    ]);
    // `Array Int` and `Array (Array a)` reach every element shape under test.
    root_types(&mut module, [TypeId(4), TypeId(5)]);
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
    let mut module = empty_module(vec![
        Type::Application(TypeId(2), TypeId(1)),
        Type::Application(TypeId(2), TypeId(0)),
        Type::Constructor(TypeConstructor::Array),
    ]);
    // The two mutually recursive arrays are unreachable without a root.
    root_types(&mut module, [TypeId(0)]);
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
fn quantifier_erasure_terminates_on_a_malformed_cycle() {
    let quantified = TypeId(0);
    let module = empty_module(vec![Type::ForAll {
        variables: vec![TypeVariableId(0)],
        body: quantified,
    }]);

    assert_eq!(
        unquantified_type(&module, quantified),
        quantified,
        "backend erasure must remain bounded even before Core verification"
    );
}

#[test]
fn bound_rank_n_record_fields_do_not_make_dictionary_layout_dependent() {
    fn push_closed_record(types: &mut Vec<Type>, field_type: TypeId) -> TypeId {
        let empty = TypeId(types.len() as u32);
        types.push(Type::RowEmpty);
        let row = TypeId(types.len() as u32);
        types.push(Type::RowExtend {
            label: "method".into(),
            ty: field_type,
            tail: empty,
        });
        let record_constructor = TypeId(types.len() as u32);
        types.push(Type::Constructor(TypeConstructor::Record));
        let record = TypeId(types.len() as u32);
        types.push(Type::Application(record_constructor, row));
        record
    }

    let variable = TypeVariableId(0);
    let mut types = vec![
        Type::Variable(variable),
        Type::Constructor(TypeConstructor::Int),
    ];
    let identity = push_arrow(&mut types, TypeId(0), TypeId(0));
    let polymorphic_identity = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![variable],
        body: identity,
    });
    let dictionary = push_closed_record(&mut types, polymorphic_identity);
    let genuinely_dependent = push_closed_record(&mut types, TypeId(0));
    let module = empty_module(types);

    fn parameter_shape(module: &Module, ty: TypeId, representation: ReprId) -> ValueShape {
        let record_types = HashMap::from([(ty, representation)]);
        super::scalar::function_parameter_shape(
            module,
            ty,
            module.span,
            &HashSet::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
            &record_types,
            &HashMap::new(),
        )
        .expect("a closed record parameter has a canonical product shape")
    }

    assert!(!depends_on_type_variable(&module, dictionary));
    assert!(depends_on_type_variable(&module, genuinely_dependent));
    for (ty, representation) in [(dictionary, ReprId(7)), (genuinely_dependent, ReprId(8))] {
        assert_eq!(
            parameter_shape(&module, ty, representation),
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(representation),
            })
        );
    }
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
        external_types: Vec::new(),
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

#[test]
fn quantified_results_keep_a_separate_callable_signature_and_arity() {
    let int = TypeId(0);
    let a = TypeId(1);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Variable(TypeVariableId(0)),
    ];
    let identity_body = push_arrow(&mut types, a, a);
    let quantified_identity = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![TypeVariableId(0)],
        body: identity_body,
    });
    let producer = push_arrow(&mut types, int, quantified_identity);
    let mut module = empty_module(types);
    module.declarations.push(Declaration {
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "makeIdentity".into(),
        name_span: psrs_span::TextRange::new(0, 1),
        quantified: Vec::new(),
        ty: producer,
        value: Expr {
            kind: ExprKind::Integer(0),
            ty: TypeId(0),
            span: psrs_span::TextRange::new(0, 1),
        },
        span: psrs_span::TextRange::new(0, 1),
    });
    let layout = layout_for(&module);

    let (parameters, result) = function_arrow_parameters(&module, producer);
    assert_eq!(parameters, vec![int]);
    assert_eq!(result, quantified_identity);
    let producer_signature = layout
        .representations
        .signature(layout.function_types[&producer]);
    let identity_signature = layout
        .representations
        .signature(layout.function_types[&quantified_identity]);
    assert_eq!(producer_signature.unwrap().parameters.len(), 1);
    assert_eq!(
        producer_signature.unwrap().result,
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Closure(layout.function_types[&quantified_identity]),
        })
    );
    assert_eq!(identity_signature.unwrap().parameters.len(), 1);
    assert_eq!(
        identity_signature.unwrap().parameters[0],
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
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
        external_types: Vec::new(),
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
    let mut module = Module {
        type_names: Vec::new(),
        id: module_id,
        name: "OpaqueHandleLayoutTest".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
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
    root_types(&mut module, [array_handle]);
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

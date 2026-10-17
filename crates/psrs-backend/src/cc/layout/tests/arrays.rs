use super::*;

#[test]
fn source_arrays_share_erased_element_storage() {
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
    assert_eq!(
        generic, concrete,
        "source arrays preserve storage through instantiation"
    );
    assert_eq!(
        generic, nested,
        "nested arrays use the same storage protocol"
    );
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
            element: crate::cc::payload::erased_shape(),
        })
    );
    assert_eq!(
        layout.representations.representation(nested),
        Some(&Representation::Array {
            element: crate::cc::payload::erased_shape(),
        })
    );
}

#[test]
fn a_wit_import_scheme_lays_out_array_payloads_without_a_declaration_root() {
    // Canonical guest projection keys a WIT import by its checked scheme TypeId.
    // That node is not always shared with a remaining declaration, so the
    // scheme itself must be a live layout root.
    let int = TypeId(0);
    let array_int = TypeId(2);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Array),
        Type::Application(TypeId(1), TypeId(0)),
    ];
    let scheme = push_arrow(&mut types, int, array_int);
    let symbol = SymbolId::new(ModuleId::INTRINSICS, 3);
    let module = Module {
        type_names: Vec::new(),
        id: ModuleId(2),
        name: "WitArrayImportLayout".into(),
        externals: vec![psrs_hir::ExternalSymbol {
            source_module: None,
            symbol,
            name: "bytes".into(),
            kind: psrs_hir::ExternalKind::Wit {
                interface: "wasi:random/random".into(),
                function: "get-random-bytes".into(),
            },
            signature: None,
        }],
        external_types: vec![psrs_core::ExternalType {
            symbol,
            source_module: ModuleId(2),
            ty: scheme,
        }],
        types,
        newtype_ids: Vec::new(),
        ordinary_data_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: psrs_span::TextRange::new(0, 8),
    };
    let layout = layout_for(&module);
    assert!(
        layout.array_types.contains_key(&array_int),
        "WIT Array Int import scheme must reserve an array layout"
    );
}

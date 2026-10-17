use super::*;

#[test]
fn open_rows_share_the_full_record_store_without_freezing_their_prefix() {
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::Variable(TypeVariableId(0)),
        Type::Variable(TypeVariableId(1)),
        Type::Constructor(TypeConstructor::Record),
        Type::RowExtend {
            label: "value".into(),
            ty: TypeId(0),
            tail: TypeId(2),
        },
        Type::Application(TypeId(4), TypeId(5)),
        Type::RowExtend {
            label: "enabled".into(),
            ty: TypeId(1),
            tail: TypeId(3),
        },
        Type::Application(TypeId(4), TypeId(7)),
        Type::Constructor(TypeConstructor::Array),
        Type::Application(TypeId(9), TypeId(0)),
    ];
    let first = push_arrow(&mut types, TypeId(6), TypeId(6));
    let second = push_arrow(&mut types, TypeId(8), TypeId(8));
    let mut module = empty_module(types);
    root_types(&mut module, [first, second, TypeId(10)]);
    let layout = layout_for(&module);
    let storage = layout.representations.record_storage().unwrap();
    assert_eq!(layout.record_types[&TypeId(6)], storage.representation());
    assert_eq!(layout.record_types[&TypeId(8)], storage.representation());
    assert_eq!(layout.array_types[&TypeId(10)], storage.values());
    assert!(
        layout
            .representations
            .product_labels(storage.representation())
            .is_none()
    );
    for callable in [first, second] {
        let signature = layout
            .representations
            .signature(layout.function_types[&callable])
            .unwrap();
        assert_eq!(
            signature.parameters,
            vec![ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(storage.representation()),
            })]
        );
        assert_eq!(signature.result, signature.parameters[0]);
    }
    super::super::super::verify_module(&super::super::super::Module {
        name: "OpenRecordLayout".into(),
        externals: Vec::new(),
        representations: layout.representations,
        functions: Vec::new(),
        entry: None,
        span: module.span,
    })
    .expect("normalized storage schema remains valid at the CC boundary");
}

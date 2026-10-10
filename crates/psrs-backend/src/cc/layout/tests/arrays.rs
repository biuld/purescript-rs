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

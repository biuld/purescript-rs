use super::*;

/// Appends a closed record as `Application(Constructor(Record), row)` and
/// returns its id.
fn push_record(types: &mut Vec<Type>, fields: Vec<(&str, TypeId)>) -> TypeId {
    let mut sorted = fields;
    sorted.sort_by(|left, right| left.0.cmp(right.0));
    let row_empty = TypeId(types.len() as u32);
    types.push(Type::RowEmpty);
    let mut tail = row_empty;
    for (label, ty) in sorted.into_iter().rev() {
        let id = TypeId(types.len() as u32);
        types.push(Type::RowExtend {
            label: label.into(),
            ty,
            tail,
        });
        tail = id;
    }
    let head = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Record));
    let id = TypeId(types.len() as u32);
    types.push(Type::Application(head, tail));
    id
}

#[test]
fn parameter_dependent_record_field_keeps_its_canonical_template_in_the_adt_slot() {
    let module_id = ModuleId(0);
    let wrap_type = HirTypeId::new(module_id, 0);
    let wrap = SymbolId::new(module_id, 0);
    let array_a = TypeId(3);
    let mut types = vec![
        Type::Constructor(TypeConstructor::User(wrap_type)),
        Type::Variable(TypeVariableId(0)),
        Type::Constructor(TypeConstructor::Array),
        Type::Application(TypeId(2), TypeId(1)),
    ];
    let record_a = push_record(&mut types, vec![("values", array_a)]);
    let module = Module {
        type_names: Vec::new(),
        id: module_id,
        name: "RecordPayloadLayoutTest".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: vec![ConstructorInfo {
            symbol: wrap,
            name: "Wrap".into(),
            type_id: wrap_type,
            tag: 0,
            field_count: 1,
            field_types: vec![record_a],
            parameters: Vec::new(),
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
                fields: vec![ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(record_repr),
                })],
            }],
        })
    );
}

#[test]
fn a_variant_case_may_carry_a_record_payload() {
    // WIT maps `datetime`, socket addresses, and directory entries to records,
    // and a variant may carry one as a case payload. The record has its own
    // representation handle, so the case field is a reference to it.
    let module_id = ModuleId(0);
    let wrap_type = HirTypeId::new(module_id, 0);
    let wrap = SymbolId::new(module_id, 0);
    let mut types = vec![Type::Constructor(psrs_core::TypeConstructor::Int)];
    let record = push_record(&mut types, vec![("value", TypeId(0))]);
    types.push(Type::Constructor(TypeConstructor::User(wrap_type)));
    let module = Module {
        type_names: Vec::new(),
        id: module_id,
        name: "RecordPayloadVariantLayoutTest".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: vec![ConstructorInfo {
            symbol: wrap,
            name: "Wrap".into(),
            type_id: wrap_type,
            tag: 0,
            field_count: 1,
            field_types: vec![record],
            parameters: Vec::new(),
        }],
        declarations: Vec::new(),
        entry: None,
        span: psrs_span::TextRange::new(0, 40),
    };
    let newtypes = HashSet::new();
    let enums = enum_type_ids(&module, &newtypes);
    let aggregates = aggregate_type_ids(&module, &newtypes);
    assert!(
        aggregates.contains(&wrap_type),
        "a variant with a record payload is an aggregate"
    );
    let layout = type_layout(&module, &enums, &aggregates, &newtypes)
        .expect("a record-payload variant should have a layout");
    let record_repr = layout.record_types[&record];
    let representation = layout.constructor_types[&wrap];
    assert_eq!(
        layout.representations.representation(representation),
        Some(&Representation::Variant {
            cases: vec![VariantCase {
                tag: 0,
                fields: vec![ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(record_repr),
                })],
            }],
        })
    );
}

#[test]
fn canonical_record_keys_sort_labels_and_share_equal_keyed_records() {
    let mut types = vec![
        Type::Variable(TypeVariableId(0)),
        Type::Constructor(psrs_core::TypeConstructor::Int),
    ];
    let first_record = push_record(&mut types, vec![("x", TypeId(0)), ("y", TypeId(1))]);
    let second_record = push_record(&mut types, vec![("y", TypeId(1)), ("x", TypeId(0))]);
    let module = empty_module(types);
    let layout = layout_for(&module);
    let first = layout.record_types[&first_record];
    let second = layout.record_types[&second_record];
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

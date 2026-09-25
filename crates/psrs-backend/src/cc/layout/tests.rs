use super::*;
use crate::cc::{RefShape, Reference};
use psrs_core::{ConstructorInfo, Module, Type, TypeConstructor};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};

#[test]
fn parameter_dependent_record_constructor_field_uses_erased_storage() {
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
    let representation = layout.constructor_types[&wrap];
    assert_eq!(
        layout.representations.representation(representation),
        Some(&Representation::Variant {
            cases: vec![VariantCase {
                tag: 0,
                fields: vec![ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Erased,
                })],
            }],
        })
    );
}

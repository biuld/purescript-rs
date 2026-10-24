use super::*;

#[test]
fn compaction_preserves_quantifiers_constructor_fields_and_expression_binders() {
    // A preceding empty module shifts every live reference across a hole.
    let original = super::link::identity(4, 0);
    let mut empty = original.clone();
    empty.types = vec![Type::Constructor(TypeConstructor::String)];
    empty.declarations.clear();
    empty.constructors.clear();
    let mut module = crate::link(vec![empty, original]);
    module.verify().unwrap();
    let before = module.clone();
    module.compact_types().unwrap();
    assert_eq!(module.types.len() + 1, before.types.len());
    assert_eq!(module.declarations[0].symbol, before.declarations[0].symbol);
    assert_eq!(
        module.declarations[0].quantified,
        before.declarations[0].quantified
    );
    assert_eq!(
        module.constructors[0].parameters,
        before.constructors[0].parameters
    );
    assert_eq!(module.declarations[0].span, before.declarations[0].span);
    let compacted = module.clone();
    module.compact_types().unwrap();
    assert_eq!(module, compacted);
}

#[test]
fn compaction_rejects_an_unused_dangling_type_without_mutation() {
    let mut module = super::link::identity(0, 0);
    module
        .types
        .push(Type::Application(TypeId(u32::MAX), TypeId(0)));
    let before = module.clone();
    let expected = module.verify().unwrap_err();
    assert_eq!(module.compact_types().unwrap_err(), expected);
    assert_eq!(module, before);
}

#[test]
fn compaction_rejects_an_unused_cycle_without_mutation() {
    let mut module = super::link::identity(0, 0);
    let cycle = TypeId(module.types.len() as u32);
    module.types.push(Type::Application(cycle, TypeId(0)));
    let before = module.clone();
    let expected = module.verify().unwrap_err();
    assert_eq!(module.compact_types().unwrap_err(), expected);
    assert_eq!(module, before);
}

#[test]
fn compaction_retains_a_foreign_signature_without_an_executable_use() {
    let mut module = super::external_types::fixture();
    module
        .types
        .push(Type::Constructor(TypeConstructor::String));
    module.compact_types().unwrap();
    assert_eq!(module.types, vec![Type::Constructor(TypeConstructor::Int)]);
    assert_eq!(module.external_types[0].ty, TypeId(0));
    assert_eq!(module.externals.len(), 1);
    module.external_types.clear();
    assert!(module.verify().is_err());
}

use super::*;
use psrs_core::{ConstructorInfo, Type};
use psrs_hir::{ModuleId, SymbolId};

const OWNER: HirTypeId = HirTypeId::new(ModuleId(0), 0);

fn module(types: Vec<Type>, field: TypeId, parameters: Vec<TypeVariableId>) -> Module {
    Module {
        id: ModuleId(0),
        name: "CallableTemplate".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types,
        newtype_ids: vec![OWNER],
        ordinary_data_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: vec![ConstructorInfo {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "Action".into(),
            type_id: OWNER,
            tag: 0,
            field_count: 1,
            field_types: vec![field],
            parameters,
        }],
        declarations: Vec::new(),
        type_names: Vec::new(),
        entry: None,
        span: psrs_span::TextRange::new(0, 1),
    }
}

#[test]
fn nested_newtype_keeps_application_evidence_and_canonical_storage_owner() {
    let inner = HirTypeId::new(ModuleId(0), 1);
    let r = TypeVariableId(0);
    let a = TypeVariableId(1);
    let outer_a = TypeVariableId(2);
    let mut module = module(
        vec![
            Type::Variable(r),
            Type::Variable(a),
            Type::Constructor(TypeConstructor::Function),
            Type::Application(TypeId(2), TypeId(0)),
            Type::Application(TypeId(3), TypeId(1)),
            Type::Constructor(TypeConstructor::User(inner)),
            Type::Constructor(TypeConstructor::Int),
            Type::Application(TypeId(5), TypeId(6)),
            Type::Variable(outer_a),
            Type::Application(TypeId(7), TypeId(8)),
        ],
        TypeId(9),
        vec![outer_a],
    );
    module.newtype_ids.push(inner);
    module.constructors.push(ConstructorInfo {
        symbol: SymbolId::new(ModuleId(0), 1),
        name: "Inner".into(),
        type_id: inner,
        tag: 0,
        field_count: 1,
        field_types: vec![TypeId(4)],
        parameters: vec![r, a],
    });
    let template = callable(&module, OWNER).unwrap();
    assert_eq!(template.source_field, TypeId(9));
    assert_eq!(template.source_variables, [outer_a]);
    assert_eq!(template.storage_owner, inner);
    assert_eq!(template.storage_field, TypeId(4));
    assert_eq!(
        template.parameters,
        [TypeId(0)],
        "canonical storage is not specialized to Int"
    );
    assert_eq!(template.result, TypeId(1));
}

#[test]
fn nullary_closure_returns_a_function_without_absorbing_its_parameters() {
    let module = module(
        vec![
            Type::Constructor(TypeConstructor::Int),
            Type::Constructor(TypeConstructor::Function),
            Type::Application(TypeId(1), TypeId(0)),
            Type::Application(TypeId(2), TypeId(0)),
            Type::Closure {
                parameters: vec![],
                result: TypeId(3),
            },
        ],
        TypeId(4),
        vec![],
    );
    let template = callable(&module, OWNER).unwrap();
    assert!(template.parameters.is_empty());
    assert_eq!(template.result, TypeId(3));
}

#[test]
fn cyclic_and_dangling_call_boundaries_do_not_become_valid_empty_protocols() {
    for (types, field) in [
        (
            vec![
                Type::Constructor(TypeConstructor::Function),
                Type::Constructor(TypeConstructor::Int),
                Type::Application(TypeId(0), TypeId(1)),
                Type::Application(TypeId(2), TypeId(3)),
            ],
            TypeId(3),
        ),
        (
            vec![
                Type::Constructor(TypeConstructor::Int),
                Type::Closure {
                    parameters: vec![TypeId(99)],
                    result: TypeId(0),
                },
            ],
            TypeId(1),
        ),
        (
            vec![Type::ForAll {
                variables: vec![TypeVariableId(0)],
                body: TypeId(0),
            }],
            TypeId(0),
        ),
    ] {
        let module = module(types, field, vec![]);
        assert!(crate::cc::checked_function_arrow_parameters(&module, field).is_err());
        assert!(callable(&module, OWNER).is_none());
    }
}

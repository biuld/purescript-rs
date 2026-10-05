//! Checked instantiation evidence. These cases cover the relation P8 reads
//! before representation lowering rewrites a constructor application.

use super::*;
use psrs_hir::{ModuleId, TypeId as HirTypeId, TypeVariableId};
use std::collections::HashMap;

const SPAN: TextRange = TextRange::new(0, 1);

fn bare(types: Vec<Type>) -> Module {
    Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Instantiation".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: SPAN,
    }
}

fn apply(types: &mut Vec<Type>, function: TypeId, argument: TypeId) -> TypeId {
    let id = TypeId(types.len() as u32);
    types.push(Type::Application(function, argument));
    id
}

fn nominal(types: &mut Vec<Type>, id: u32, argument: TypeId) -> TypeId {
    let constructor = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::User(HirTypeId::new(
        ModuleId(0),
        id,
    ))));
    apply(types, constructor, argument)
}

#[test]
fn distinct_nominal_applications_are_rejected() {
    let mut types = vec![Type::Constructor(TypeConstructor::Int)];
    let maybe = nominal(&mut types, 1, TypeId(0));
    let either = nominal(&mut types, 2, TypeId(0));
    let module = bare(types);
    assert!(module.checked_instantiation(maybe, &[], either).is_none());
}

#[test]
fn flexible_variables_match_from_either_side() {
    let variable = TypeVariableId(7);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Variable(variable),
    ];
    let head = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::User(HirTypeId::new(
        ModuleId(0),
        1,
    ))));
    let concrete = apply(&mut types, head, TypeId(0));
    let abstract_application = apply(&mut types, TypeId(1), TypeId(0));
    let module = bare(types);

    let from_scheme = module
        .checked_instantiation(abstract_application, &[variable], concrete)
        .expect("a flexible head instantiates to the nominal constructor");
    assert_eq!(
        from_scheme.constructor(variable),
        Some((
            TypeConstructor::User(HirTypeId::new(ModuleId(0), 1)),
            Vec::new()
        ))
    );

    let from_target = module
        .checked_instantiation(concrete, &[variable], abstract_application)
        .expect("a flexible target head binds to the concrete constructor");
    assert_eq!(
        from_target.constructor(variable),
        Some((
            TypeConstructor::User(HirTypeId::new(ModuleId(0), 1)),
            Vec::new()
        ))
    );
}

#[test]
fn dictionary_fields_alpha_rename_their_quantifiers() {
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Variable(TypeVariableId(1)),
        Type::Variable(TypeVariableId(2)),
        Type::RowEmpty,
    ];
    let left_body = arrow(&mut types, TypeId(0), TypeId(1));
    let left_scheme = forall(&mut types, vec![TypeVariableId(1)], left_body);
    let right_body = arrow(&mut types, TypeId(0), TypeId(2));
    let right_scheme = forall(&mut types, vec![TypeVariableId(2)], right_body);
    let left = record(&mut types, left_scheme);
    let right = record(&mut types, right_scheme);
    let module = bare(types);
    assert!(
        module.checked_instantiation(left, &[], right).is_some(),
        "a dictionary field quantifier is alpha-equivalent under a renamed binder"
    );
}

#[test]
fn constructor_binding_rejects_unsolved_variables_and_cycles() {
    let variable = TypeVariableId(3);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Variable(variable),
        Type::Constructor(TypeConstructor::Function),
    ];
    let function_int = apply(&mut types, TypeId(2), TypeId(0));
    let solved_module = bare(types);
    let solved = Instantiation {
        module: &solved_module,
        replacements: HashMap::from([(variable, function_int)]),
    };
    assert_eq!(
        solved.constructor(variable),
        Some((TypeConstructor::Function, vec![TypeId(0)]))
    );

    let unsolved = Instantiation {
        module: &solved_module,
        replacements: HashMap::from([(variable, TypeId(1))]),
    };
    assert_eq!(unsolved.constructor(variable), None);

    let cyclic_module = bare(vec![Type::Application(TypeId(0), TypeId(0))]);
    let cyclic = Instantiation {
        module: &cyclic_module,
        replacements: HashMap::from([(variable, TypeId(0))]),
    };
    assert_eq!(cyclic.constructor(variable), None);
}

fn arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let function = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Function));
    let applied = apply(types, function, parameter);
    apply(types, applied, result)
}

fn forall(types: &mut Vec<Type>, variables: Vec<TypeVariableId>, body: TypeId) -> TypeId {
    let id = TypeId(types.len() as u32);
    types.push(Type::ForAll { variables, body });
    id
}

fn record(types: &mut Vec<Type>, field: TypeId) -> TypeId {
    let row = TypeId(types.len() as u32);
    types.push(Type::RowExtend {
        label: "chain".into(),
        ty: field,
        tail: TypeId(3),
    });
    let head = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Record));
    apply(types, head, row)
}

#[test]
fn type_level_literals_match_by_value_in_invariant_applications() {
    let mut types = vec![
        Type::TypeLevelString("Pair".into()),
        Type::TypeLevelString("Pair".into()),
        Type::TypeLevelString("Single".into()),
        Type::TypeLevelInt(42),
        Type::TypeLevelInt(42),
        Type::TypeLevelInt(43),
    ];
    for (first, equal, different) in [(0, 1, 2), (3, 4, 5)] {
        let first = nominal(&mut types, 0, TypeId(first));
        let equal = nominal(&mut types, 0, TypeId(equal));
        let different = nominal(&mut types, 0, TypeId(different));
        let module = bare(types.clone());
        assert!(module.checked_instantiation(first, &[], equal).is_some());
        assert!(
            module
                .checked_instantiation(first, &[], different)
                .is_none()
        );
    }
}

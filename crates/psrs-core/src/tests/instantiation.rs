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
        rows: HashMap::new(),
        replacements: HashMap::from([(variable, function_int)]),
    };
    assert_eq!(
        solved.constructor(variable),
        Some((TypeConstructor::Function, vec![TypeId(0)]))
    );

    let unsolved = Instantiation {
        module: &solved_module,
        rows: HashMap::new(),
        replacements: HashMap::from([(variable, TypeId(1))]),
    };
    assert_eq!(unsolved.constructor(variable), None);

    let cyclic_module = bare(vec![Type::Application(TypeId(0), TypeId(0))]);
    let cyclic = Instantiation {
        module: &cyclic_module,
        rows: HashMap::new(),
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

#[test]
fn row_evidence_retains_residuals_without_an_arena_node() {
    let variable = TypeVariableId(40);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::RowEmpty,
        Type::Variable(variable),
        Type::Constructor(TypeConstructor::Record),
    ];
    let generic_row = TypeId(types.len() as u32);
    types.push(Type::RowExtend {
        label: "b".into(),
        ty: TypeId(0),
        tail: TypeId(2),
    });
    let generic = apply(&mut types, TypeId(3), generic_row);
    let mut row = TypeId(1);
    for label in ["c", "b", "a"] {
        let id = TypeId(types.len() as u32);
        types.push(Type::RowExtend {
            label: label.into(),
            ty: TypeId(0),
            tail: row,
        });
        row = id;
    }
    let concrete = apply(&mut types, TypeId(3), row);
    let module = bare(types);
    let evidence = module
        .checked_instantiation(generic, &[variable], concrete)
        .unwrap();
    let residual = evidence.row(variable).unwrap();
    assert_eq!(
        residual.fields,
        vec![("a".into(), TypeId(0)), ("c".into(), TypeId(0))]
    );
    assert_eq!(residual.tail, None);
    assert_eq!(evidence.constructor(variable), None);
    assert_eq!(evidence.row(TypeVariableId(41)).map(|row| row.fields), None);
}

fn row(types: &mut Vec<Type>, fields: &[(&str, TypeId)], tail: TypeId) -> TypeId {
    fields.iter().rev().fold(tail, |tail, (label, ty)| {
        let id = TypeId(types.len() as u32);
        types.push(Type::RowExtend {
            label: (*label).into(),
            ty: *ty,
            tail,
        });
        id
    })
}

#[test]
fn nominal_row_arguments_preserve_checked_residuals_and_label_order_independence() {
    let variable = TypeVariableId(40);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::RowEmpty,
        Type::Variable(variable),
    ];
    let generic_row = row(&mut types, &[("y", TypeId(0))], TypeId(3));
    let generic = nominal(&mut types, 1, generic_row);
    let concrete_row = row(
        &mut types,
        &[("a", TypeId(0)), ("b", TypeId(1)), ("y", TypeId(0))],
        TypeId(2),
    );
    let concrete = nominal(&mut types, 1, concrete_row);
    let reordered_row = row(
        &mut types,
        &[("y", TypeId(0)), ("b", TypeId(1)), ("a", TypeId(0))],
        TypeId(2),
    );
    let reordered = nominal(&mut types, 1, reordered_row);
    let generic_arrow = arrow(&mut types, generic, generic);
    let consistent_arrow = arrow(&mut types, concrete, reordered);
    let different_row = row(
        &mut types,
        &[("y", TypeId(0)), ("b", TypeId(0)), ("a", TypeId(0))],
        TypeId(2),
    );
    let different = nominal(&mut types, 1, different_row);
    let inconsistent_arrow = arrow(&mut types, concrete, different);
    let missing_row = row(&mut types, &[("a", TypeId(0))], TypeId(2));
    let missing = nominal(&mut types, 1, missing_row);
    let duplicate_row = row(&mut types, &[("y", TypeId(0)), ("y", TypeId(0))], TypeId(2));
    let duplicate = nominal(&mut types, 1, duplicate_row);
    let module = bare(types);
    let evidence = module
        .checked_instantiation(generic_arrow, &[variable], consistent_arrow)
        .unwrap();
    let mut residual = evidence.row(variable).unwrap();
    residual.fields.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        residual.fields,
        vec![("a".into(), TypeId(0)), ("b".into(), TypeId(1))]
    );
    assert_eq!(residual.tail, None);
    assert!(
        module
            .checked_instantiation(concrete, &[], reordered)
            .is_some()
    );
    assert!(
        module
            .checked_instantiation(generic_arrow, &[variable], inconsistent_arrow)
            .is_none()
    );
    assert!(
        module
            .checked_instantiation(generic, &[], concrete)
            .is_none(),
        "rigid tails cannot absorb fields"
    );
    assert!(
        module
            .checked_instantiation(generic, &[variable], missing)
            .is_none()
    );
    let duplicate_evidence = module
        .checked_instantiation(generic, &[variable], duplicate)
        .unwrap();
    assert_eq!(
        duplicate_evidence.row(variable).unwrap().fields,
        vec![("y".into(), TypeId(0))]
    );
}

#[test]
fn nominal_row_arguments_preserve_duplicate_label_occurrence_order() {
    let variable = TypeVariableId(41);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::RowEmpty,
        Type::Variable(variable),
    ];
    let scheme_row = row(&mut types, &[("a", TypeId(0)), ("a", TypeId(1))], TypeId(3));
    let scheme = nominal(&mut types, 1, scheme_row);
    let matching_row = row(
        &mut types,
        &[("z", TypeId(0)), ("a", TypeId(0)), ("a", TypeId(1))],
        TypeId(2),
    );
    let matching = nominal(&mut types, 1, matching_row);
    let reversed_row = row(
        &mut types,
        &[("a", TypeId(1)), ("a", TypeId(0)), ("z", TypeId(0))],
        TypeId(2),
    );
    let reversed = nominal(&mut types, 1, reversed_row);
    let module = bare(types);
    let evidence = module
        .checked_instantiation(scheme, &[variable], matching)
        .unwrap();
    assert_eq!(
        evidence.row(variable).unwrap().fields,
        vec![("z".into(), TypeId(0))]
    );
    assert!(
        module
            .checked_instantiation(scheme, &[variable], reversed)
            .is_none()
    );
}

#[test]
fn repeated_residual_fields_use_the_checked_relation_for_nested_row_arguments() {
    let variable = TypeVariableId(42);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::RowEmpty,
        Type::Variable(variable),
    ];
    let nested_left = row(&mut types, &[("a", TypeId(0)), ("b", TypeId(1))], TypeId(2));
    let nested_left = nominal(&mut types, 2, nested_left);
    let nested_right = row(&mut types, &[("b", TypeId(1)), ("a", TypeId(0))], TypeId(2));
    let nested_right = nominal(&mut types, 2, nested_right);
    let scheme_row = row(&mut types, &[("y", TypeId(0))], TypeId(3));
    let scheme = nominal(&mut types, 1, scheme_row);
    let scheme = arrow(&mut types, scheme, scheme);
    let domain_row = row(
        &mut types,
        &[("x", nested_left), ("y", TypeId(0))],
        TypeId(2),
    );
    let domain = nominal(&mut types, 1, domain_row);
    let codomain_row = row(
        &mut types,
        &[("y", TypeId(0)), ("x", nested_right)],
        TypeId(2),
    );
    let codomain = nominal(&mut types, 1, codomain_row);
    let instance = arrow(&mut types, domain, codomain);
    let module = bare(types);
    assert!(
        module
            .checked_instantiation(scheme, &[variable], instance)
            .is_some()
    );
}

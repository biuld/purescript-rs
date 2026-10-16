use super::*;
use crate::{Expr, ExprKind, TypeConstructor};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId, TypeVariableId};
use psrs_span::TextRange;

fn push(types: &mut Vec<Type>, ty: Type) -> TypeId {
    let id = TypeId(types.len() as u32);
    types.push(ty);
    id
}

fn row(types: &mut Vec<Type>, fields: &[(&str, TypeId)], tail: TypeId) -> TypeId {
    fields.iter().rev().fold(tail, |tail, (label, ty)| {
        push(
            types,
            Type::RowExtend {
                label: (*label).into(),
                ty: *ty,
                tail,
            },
        )
    })
}

fn proxy(types: &mut Vec<Type>, row: TypeId) -> TypeId {
    let constructor = push(
        types,
        Type::Constructor(TypeConstructor::User(HirTypeId::new(ModuleId(0), 0))),
    );
    push(types, Type::Application(constructor, row))
}

fn arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let function = push(types, Type::Constructor(TypeConstructor::Function));
    let applied = push(types, Type::Application(function, parameter));
    push(types, Type::Application(applied, result))
}

fn module(types: Vec<Type>, declarations: Vec<crate::Declaration>) -> Module {
    Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations,
        type_names: Vec::new(),
        span: TextRange::new(0, 0),
    }
}

#[test]
fn scheme_instantiation_matches_reordered_rows_inside_proxy_types() {
    let mut types = vec![Type::RowEmpty];
    let empty = TypeId(0);
    let int = push(&mut types, Type::Constructor(TypeConstructor::Int));
    let boolean = push(&mut types, Type::Constructor(TypeConstructor::Boolean));
    let tail_variable = TypeVariableId(40);
    let tail = push(&mut types, Type::Variable(tail_variable));
    let scheme_row = row(&mut types, &[("y", int)], tail);
    let scheme = proxy(&mut types, scheme_row);
    let actual_row = row(&mut types, &[("a", boolean), ("y", int)], empty);
    let actual = proxy(&mut types, actual_row);
    let module = module(types, Vec::new());

    assert!(scheme_instance(scheme, &[tail_variable], actual, &module));
}

#[test]
fn scheme_instantiation_preserves_duplicate_label_occurrence_order() {
    let mut types = vec![Type::RowEmpty];
    let empty = TypeId(0);
    let int = push(&mut types, Type::Constructor(TypeConstructor::Int));
    let boolean = push(&mut types, Type::Constructor(TypeConstructor::Boolean));
    let string = push(&mut types, Type::Constructor(TypeConstructor::String));
    let tail_variable = TypeVariableId(41);
    let tail = push(&mut types, Type::Variable(tail_variable));
    let scheme_row = row(&mut types, &[("a", int), ("a", boolean)], tail);
    let scheme = proxy(&mut types, scheme_row);
    let matching_row = row(
        &mut types,
        &[("a", int), ("a", boolean), ("z", string)],
        empty,
    );
    let matching = proxy(&mut types, matching_row);
    let reversed_row = row(
        &mut types,
        &[("a", boolean), ("a", int), ("z", string)],
        empty,
    );
    let reversed = proxy(&mut types, reversed_row);
    let module = module(types, Vec::new());

    assert!(scheme_instance(scheme, &[tail_variable], matching, &module));
    assert!(!scheme_instance(
        scheme,
        &[tail_variable],
        reversed,
        &module
    ));
}

#[test]
fn scheme_instantiation_matches_reordered_rows_inside_record_types() {
    let mut types = vec![Type::RowEmpty];
    let empty = TypeId(0);
    let int = push(&mut types, Type::Constructor(TypeConstructor::Int));
    let boolean = push(&mut types, Type::Constructor(TypeConstructor::Boolean));
    let record = push(&mut types, Type::Constructor(TypeConstructor::Record));
    let tail_variable = TypeVariableId(42);
    let tail = push(&mut types, Type::Variable(tail_variable));
    let scheme_row = row(&mut types, &[("y", int)], tail);
    let scheme = push(&mut types, Type::Application(record, scheme_row));
    let actual_row = row(&mut types, &[("a", boolean), ("y", int)], empty);
    let actual = push(&mut types, Type::Application(record, actual_row));
    let module = module(types, Vec::new());

    assert!(scheme_instance(scheme, &[tail_variable], actual, &module));
}

#[test]
fn alpha_equal_foralls_match_reordered_rows_inside_nested_proxy_types() {
    let mut types = vec![Type::RowEmpty];
    let int = push(&mut types, Type::Constructor(TypeConstructor::Int));
    let boolean = push(&mut types, Type::Constructor(TypeConstructor::Boolean));
    let left_variable = TypeVariableId(45);
    let left_tail = push(&mut types, Type::Variable(left_variable));
    let left_row = row(&mut types, &[("a", int), ("z", boolean)], left_tail);
    let left_inner_proxy = proxy(&mut types, left_row);
    let left_body = proxy(&mut types, left_inner_proxy);
    let left = push(
        &mut types,
        Type::ForAll {
            variables: vec![left_variable],
            body: left_body,
        },
    );

    let right_variable = TypeVariableId(46);
    let right_tail = push(&mut types, Type::Variable(right_variable));
    let right_row = row(&mut types, &[("z", boolean), ("a", int)], right_tail);
    let right_inner_proxy = proxy(&mut types, right_row);
    let right_body = proxy(&mut types, right_inner_proxy);
    let right = push(
        &mut types,
        Type::ForAll {
            variables: vec![right_variable],
            body: right_body,
        },
    );
    let module = module(types, Vec::new());

    assert!(Matcher::new(&module).equal(left, right, &mut HashSet::new()));
}

#[test]
fn a_more_polymorphic_function_matches_a_rank_n_instance_method() {
    let mut types = Vec::new();
    let x = TypeVariableId(50);
    let y = TypeVariableId(51);
    let z = TypeVariableId(52);
    let x_ty = push(&mut types, Type::Variable(x));
    let y_ty = push(&mut types, Type::Variable(y));
    let z_ty = push(&mut types, Type::Variable(z));
    let y_to_z = arrow(&mut types, y_ty, z_ty);
    let x_to_y = arrow(&mut types, x_ty, y_ty);
    let x_to_z = arrow(&mut types, x_ty, z_ty);
    let function_after_first = arrow(&mut types, x_to_y, x_to_z);
    let actual_body = arrow(&mut types, y_to_z, function_after_first);
    let actual = push(
        &mut types,
        Type::ForAll {
            variables: vec![x, y, z],
            body: actual_body,
        },
    );

    let a = TypeVariableId(53);
    let b = TypeVariableId(54);
    let r = TypeVariableId(55);
    let a_ty = push(&mut types, Type::Variable(a));
    let b_ty = push(&mut types, Type::Variable(b));
    let r_ty = push(&mut types, Type::Variable(r));
    let a_to_b = arrow(&mut types, a_ty, b_ty);
    let r_to_a = arrow(&mut types, r_ty, a_ty);
    let r_to_b = arrow(&mut types, r_ty, b_ty);
    let function_after_first = arrow(&mut types, r_to_a, r_to_b);
    let expected_body = arrow(&mut types, a_to_b, function_after_first);
    let expected = push(
        &mut types,
        Type::ForAll {
            variables: vec![a, b],
            body: expected_body,
        },
    );
    let module = module(types, Vec::new());

    assert!(compatible(actual, expected, &module));
}

#[test]
fn scheme_instantiation_rejects_inconsistent_reuses_of_a_generic_row_tail() {
    let mut types = vec![Type::RowEmpty];
    let empty = TypeId(0);
    let int = push(&mut types, Type::Constructor(TypeConstructor::Int));
    let boolean = push(&mut types, Type::Constructor(TypeConstructor::Boolean));
    let string = push(&mut types, Type::Constructor(TypeConstructor::String));
    let number = push(&mut types, Type::Constructor(TypeConstructor::Number));
    let tail_variable = TypeVariableId(43);
    let tail = push(&mut types, Type::Variable(tail_variable));
    let first_pattern = row(&mut types, &[("a", int)], tail);
    let first_parameter = proxy(&mut types, first_pattern);
    let second_pattern = row(&mut types, &[("b", boolean)], tail);
    let second_parameter = proxy(&mut types, second_pattern);
    let result = push(&mut types, Type::Constructor(TypeConstructor::Int));
    let rest = arrow(&mut types, second_parameter, result);
    let scheme = arrow(&mut types, first_parameter, rest);

    let first_actual_row = row(&mut types, &[("a", int), ("x", string)], empty);
    let first_actual = proxy(&mut types, first_actual_row);
    let second_actual_row = row(&mut types, &[("b", boolean), ("y", number)], empty);
    let second_actual = proxy(&mut types, second_actual_row);
    let actual_rest = arrow(&mut types, second_actual, result);
    let actual = arrow(&mut types, first_actual, actual_rest);
    let module = module(types, Vec::new());

    assert!(!scheme_instance(scheme, &[tail_variable], actual, &module));
}

#[test]
fn verifier_rejects_a_global_use_that_reuses_one_row_tail_inconsistently() {
    let mut types = vec![Type::RowEmpty];
    let empty = TypeId(0);
    let int = push(&mut types, Type::Constructor(TypeConstructor::Int));
    let boolean = push(&mut types, Type::Constructor(TypeConstructor::Boolean));
    let string = push(&mut types, Type::Constructor(TypeConstructor::String));
    let number = push(&mut types, Type::Constructor(TypeConstructor::Number));
    let tail_variable = TypeVariableId(44);
    let tail = push(&mut types, Type::Variable(tail_variable));
    let first_pattern = row(&mut types, &[("a", int)], tail);
    let first_parameter = proxy(&mut types, first_pattern);
    let second_pattern = row(&mut types, &[("b", boolean)], tail);
    let second_parameter = proxy(&mut types, second_pattern);
    let result = push(&mut types, Type::Constructor(TypeConstructor::Int));
    let scheme_rest = arrow(&mut types, second_parameter, result);
    let scheme = arrow(&mut types, first_parameter, scheme_rest);

    let first_actual_row = row(&mut types, &[("a", int), ("x", string)], empty);
    let first_actual = proxy(&mut types, first_actual_row);
    let second_actual_row = row(&mut types, &[("b", boolean), ("y", number)], empty);
    let second_actual = proxy(&mut types, second_actual_row);
    let actual_rest = arrow(&mut types, second_actual, result);
    let actual = arrow(&mut types, first_actual, actual_rest);
    let symbol = SymbolId::new(ModuleId(0), 0);
    let span = TextRange::new(0, 4);
    let value = |ty| Expr {
        kind: ExprKind::Global(symbol),
        ty,
        span,
    };
    let module = module(
        types,
        vec![
            crate::Declaration {
                symbol,
                name: "poly".into(),
                name_span: span,
                quantified: vec![tail_variable],
                ty: scheme,
                value: value(scheme),
                span,
            },
            crate::Declaration {
                symbol: SymbolId::new(ModuleId(0), 1),
                name: "badUse".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: result,
                value: value(actual),
                span,
            },
        ],
    );

    let errors = module.verify().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| { error.message == "global reference is not a valid scheme instance" })
    );
}

//! Malformed control calls must fail before their domains are erased.
use super::*;

fn append(types: &mut Vec<Type>, ty: Type) -> TypeId {
    let id = TypeId(types.len() as u32);
    types.push(ty);
    id
}

fn application(types: &mut Vec<Type>, head: TypeId, argument: TypeId) -> TypeId {
    append(types, Type::Application(head, argument))
}

fn fixture(wrong_raw: bool, wrong_target: bool) -> Module {
    let variable = psrs_hir::TypeVariableId(42);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::Constructor(TypeConstructor::User(psrs_hir::TypeId::CONTROL)),
        Type::Constructor(TypeConstructor::User(psrs_hir::TypeId::new(ModuleId(0), 7))),
        Type::Constructor(TypeConstructor::User(psrs_hir::TypeId::new(ModuleId(0), 8))),
        Type::Variable(variable),
    ];
    let head = application(&mut types, TypeId(2), TypeId(3));
    let source = application(&mut types, head, TypeId(0));
    let head = application(&mut types, TypeId(2), TypeId(4));
    let target = application(&mut types, head, TypeId(1));
    let ret = arrow_type(&mut types, TypeId(0), target);
    let request = application(&mut types, TypeId(3), TypeId(5));
    let raw = arrow_type(
        &mut types,
        if wrong_raw { TypeId(1) } else { TypeId(5) },
        source,
    );
    let result = if wrong_target { source } else { target };
    let op = arrow_type(&mut types, raw, result);
    let op = arrow_type(&mut types, request, op);
    let op = append(
        &mut types,
        Type::ForAll {
            variables: vec![variable],
            body: op,
        },
    );
    let trap = |ty| Expr {
        kind: ExprKind::Trap,
        ty,
        span: TextRange::new(3, 9),
    };
    single_declaration(
        types,
        target,
        Expr {
            kind: ExprKind::IntrinsicCall {
                intrinsic: Intrinsic::ControlHandle,
                arguments: vec![trap(ret), trap(op), trap(source)],
            },
            ty: target,
            span: TextRange::new(3, 9),
        },
    )
}

#[test]
fn verifier_accepts_distinct_control_source_and_target_answers() {
    fixture(false, false).verify().unwrap();
}

#[test]
fn verifier_rejects_control_domain_and_raw_answer_mismatches() {
    for (raw, target) in [(true, false), (false, true)] {
        let errors = fixture(raw, target).verify().unwrap_err();
        assert!(errors.iter().any(|error| error.message
            == "control handle clauses do not preserve their checked source and target contracts"));
        assert!(errors.iter().all(|error| error.module == ModuleId(0)));
    }
}

#[test]
fn verifier_rejects_old_control_arity_without_indexing_invalid_arguments() {
    for count in [0, 1, 2, 4, 5] {
        let mut module = fixture(false, false);
        let ExprKind::IntrinsicCall { arguments, .. } = &mut module.declarations[0].value.kind
        else {
            unreachable!()
        };
        arguments.resize(count, arguments[0].clone());
        assert!(has_message(
            &module,
            "intrinsic call does not have its registered saturated arity"
        ));
    }
}

#[test]
fn verifier_rejects_a_forged_source_payload_at_the_common_constructor() {
    let mut module = fixture(false, false);
    let ExprKind::IntrinsicCall { arguments, .. } = &mut module.declarations[0].value.kind else {
        unreachable!()
    };
    arguments[2].kind = ExprKind::IntrinsicCall {
        intrinsic: Intrinsic::ControlReturn,
        arguments: vec![Expr {
            kind: ExprKind::Boolean(true),
            ty: TypeId(1),
            span: TextRange::new(3, 9),
        }],
    };
    assert!(has_message(
        &module,
        "control primitive does not preserve its checked carrier and callable contracts"
    ));
}

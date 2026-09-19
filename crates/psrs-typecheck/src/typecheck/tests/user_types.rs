use super::*;

fn named(id: u32, start: u32) -> HirType {
    HirType {
        kind: HirTypeKind::Named(psrs_hir::TypeId::new(ModuleId(0), id)),
        span: TextRange::new(start, start + 1),
    }
}

fn builtin(builtin: psrs_hir::BuiltinType, start: u32) -> HirType {
    HirType {
        kind: HirTypeKind::Constructor(builtin),
        span: TextRange::new(start, start + 1),
    }
}

fn applied(function: HirType, argument: HirType, start: u32, end: u32) -> HirType {
    HirType {
        kind: HirTypeKind::Application(Box::new(function), Box::new(argument)),
        span: TextRange::new(start, end),
    }
}

fn identity_lambda(start: u32) -> HirExpr {
    expr(
        HirExprKind::Lambda {
            binder: LocalBinder {
                id: LocalId(0),
                name: "x".into(),
                span: TextRange::new(start, start + 1),
            },
            body: Box::new(local(0, start + 3)),
        },
        start - 1,
        start + 4,
    )
}

fn variable(name: &str, start: u32) -> HirType {
    HirType {
        kind: HirTypeKind::Variable(name.into()),
        span: TextRange::new(start, start + name.len() as u32),
    }
}

fn forall(variables: Vec<(&str, u32)>, body: HirType, start: u32) -> HirType {
    let end = body.span.end;
    HirType {
        kind: HirTypeKind::Forall {
            variables: variables
                .into_iter()
                .map(|(name, span)| psrs_hir::TypeParameter {
                    name: name.into(),
                    name_span: TextRange::new(span, span + 1),
                    kind: None,
                })
                .collect(),
            body: Box::new(body),
        },
        span: TextRange::new(start, end),
    }
}

#[test]
fn accepts_higher_kinded_variable_application() {
    let f_a = applied(variable("f", 30), variable("a", 32), 30, 33);
    let signature = forall(
        vec![("f", 20), ("a", 22)],
        HirType {
            kind: HirTypeKind::Function {
                parameter: Box::new(f_a.clone()),
                result: Box::new(f_a),
            },
            span: TextRange::new(28, 34),
        },
        19,
    );
    let identity = declaration_with_signature(0, "identity", 40, signature, identity_lambda(60));
    let resolved = module(vec![identity], false);
    typecheck_module(resolved).unwrap();
}

#[test]
fn rejects_mismatched_higher_kinded_application() {
    let f_a = applied(variable("f", 30), variable("a", 32), 30, 33);
    let f_int = applied(
        variable("f", 38),
        builtin(psrs_hir::BuiltinType::Int, 40),
        38,
        43,
    );
    let signature = forall(
        vec![("f", 20), ("a", 22)],
        HirType {
            kind: HirTypeKind::Function {
                parameter: Box::new(f_a),
                result: Box::new(f_int),
            },
            span: TextRange::new(28, 44),
        },
        19,
    );
    let identity = declaration_with_signature(0, "identity", 50, signature, identity_lambda(70));
    let resolved = module(vec![identity], false);
    let errors = typecheck_module(resolved).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::TypeMismatch),
        "{errors:?}"
    );
}

#[test]
fn accepts_user_type_constructors_in_signatures() {
    let int = builtin(psrs_hir::BuiltinType::Int, 30);
    let maybe_int = applied(named(0, 20), int, 20, 30);
    let identity = declaration_with_signature(
        0,
        "identity",
        19,
        HirType {
            kind: HirTypeKind::Function {
                parameter: Box::new(maybe_int.clone()),
                result: Box::new(maybe_int),
            },
            span: TextRange::new(20, 40),
        },
        identity_lambda(45),
    );
    let resolved = module(vec![identity], false);

    let typed = typecheck_module(resolved).unwrap();
    assert!(
        typed
            .types
            .iter()
            .any(|ty| matches!(ty, Type::Constructor(thir::TypeConstructor::User(_))))
    );
    assert!(
        typed
            .types
            .iter()
            .any(|ty| matches!(ty, Type::Application(_, _)))
    );
    typed.verify().unwrap();
}

#[test]
fn accepts_array_in_signatures() {
    let array_int = applied(
        builtin(psrs_hir::BuiltinType::Array, 20),
        builtin(psrs_hir::BuiltinType::Int, 26),
        20,
        29,
    );
    let identity = declaration_with_signature(
        0,
        "identity",
        19,
        HirType {
            kind: HirTypeKind::Function {
                parameter: Box::new(array_int.clone()),
                result: Box::new(array_int),
            },
            span: TextRange::new(20, 40),
        },
        identity_lambda(45),
    );
    let resolved = module(vec![identity], false);

    let typed = typecheck_module(resolved).unwrap();
    assert!(
        typed
            .types
            .iter()
            .any(|ty| matches!(ty, Type::Constructor(thir::TypeConstructor::Array)))
    );
    typed.verify().unwrap();
}

#[test]
fn rejects_distinct_type_constructors() {
    let parameter = applied(
        named(0, 20),
        builtin(psrs_hir::BuiltinType::Int, 26),
        20,
        29,
    );
    let result = applied(
        named(1, 34),
        builtin(psrs_hir::BuiltinType::Int, 40),
        34,
        43,
    );
    let identity = declaration_with_signature(
        0,
        "identity",
        19,
        HirType {
            kind: HirTypeKind::Function {
                parameter: Box::new(parameter),
                result: Box::new(result),
            },
            span: TextRange::new(20, 45),
        },
        identity_lambda(50),
    );
    let resolved = module(vec![identity], false);

    let errors = typecheck_module(resolved).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::TypeMismatch),
        "{errors:?}"
    );
}

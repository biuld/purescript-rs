use super::*;

fn ty(kind: HirTypeKind, start: u32, end: u32) -> HirType {
    HirType {
        kind,
        span: TextRange::new(start, end),
    }
}

fn function(parameter: HirType, result: HirType, span: TextRange) -> HirType {
    ty(
        HirTypeKind::Function {
            parameter: Box::new(parameter),
            result: Box::new(result),
        },
        span.start,
        span.end,
    )
}

fn forall(name: &str, body: HirType, span: TextRange) -> HirType {
    ty(
        HirTypeKind::Forall {
            variables: vec![psrs_hir::TypeParameter {
                name: name.into(),
                name_span: TextRange::new(span.start, span.start + name.len() as u32),
                kind: None,
            }],
            body: Box::new(body),
        },
        span.start,
        span.end,
    )
}

fn int_type(span: TextRange) -> HirType {
    ty(
        HirTypeKind::Constructor(psrs_hir::BuiltinType::Int),
        span.start,
        span.end,
    )
}

fn local_binder(id: u32, name: &str, start: u32) -> LocalBinder {
    LocalBinder {
        id: LocalId(id),
        name: name.into(),
        span: TextRange::new(start, start + name.len() as u32),
    }
}

fn polymorphic_identity_argument() -> HirExpr {
    expr(
        HirExprKind::Lambda {
            binder: local_binder(1, "x", 70),
            body: Box::new(local(1, 74)),
        },
        69,
        75,
    )
}

fn rank_two_use_declaration() -> HirDeclaration {
    let variable = type_variable("a", 35);
    let poly_identity = forall(
        "a",
        function(variable.clone(), variable, TextRange::new(35, 47)),
        TextRange::new(28, 47),
    );
    let signature = function(
        poly_identity,
        int_type(TextRange::new(51, 54)),
        TextRange::new(28, 54),
    );
    declaration_with_signature(
        0,
        "use",
        19,
        signature,
        expr(
            HirExprKind::Lambda {
                binder: local_binder(0, "f", 62),
                body: Box::new(integer("1", 67)),
            },
            61,
            68,
        ),
    )
}

#[test]
fn checks_rank_two_argument_and_keeps_the_forall_structural() {
    let use_declaration = rank_two_use_declaration();
    let main = declaration_with_signature(
        1,
        "main",
        80,
        int_type(TextRange::new(88, 91)),
        expr(
            HirExprKind::Application(
                Box::new(expr(
                    HirExprKind::Global(SymbolId::new(ModuleId(0), 0)),
                    95,
                    98,
                )),
                Box::new(polymorphic_identity_argument()),
            ),
            95,
            105,
        ),
    );
    let module = module(vec![use_declaration, main], false);
    let module = psrs_desugar::desugar_module(module).unwrap();
    let typed = typecheck_module(module).expect("rank-two argument should check");
    assert!(
        typed
            .types
            .iter()
            .any(|ty| matches!(ty, Type::ForAll { .. }))
    );
    typed.verify().unwrap();
}

#[test]
fn rejects_a_monomorphic_lambda_at_a_polymorphic_argument() {
    let use_declaration = rank_two_use_declaration();
    let bad_argument = expr(
        HirExprKind::Lambda {
            binder: local_binder(1, "x", 70),
            body: Box::new(integer("42", 74)),
        },
        69,
        76,
    );
    let bad = declaration(
        1,
        "bad",
        80,
        expr(
            HirExprKind::Application(
                Box::new(expr(
                    HirExprKind::Global(SymbolId::new(ModuleId(0), 0)),
                    86,
                    89,
                )),
                Box::new(bad_argument),
            ),
            86,
            97,
        ),
    );
    let errors = typecheck_module(module(vec![use_declaration, bad], false)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::TypeMismatch),
        "expected a mismatch when a monomorphic lambda is checked against forall"
    );
}

#[test]
fn rejects_an_inferred_parameter_that_would_capture_a_skolem() {
    let use_declaration = rank_two_use_declaration();
    let inferred = declaration(
        1,
        "test",
        80,
        expr(
            HirExprKind::Lambda {
                binder: local_binder(1, "x", 85),
                body: Box::new(expr(
                    HirExprKind::Application(
                        Box::new(expr(
                            HirExprKind::Global(SymbolId::new(ModuleId(0), 0)),
                            90,
                            93,
                        )),
                        Box::new(local(1, 94)),
                    ),
                    90,
                    95,
                )),
            },
            84,
            95,
        ),
    );
    let errors = typecheck_module(module(vec![use_declaration, inferred], false)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::SkolemEscape),
        "expected an escaped-skolem diagnostic, found {errors:?}"
    );
}

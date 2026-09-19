use super::*;

fn var_pattern(id: u32, name: &str, span: u32) -> psrs_hir::Pattern {
    psrs_hir::Pattern {
        kind: psrs_hir::PatternKind::Var(LocalBinder {
            id: LocalId(id),
            name: name.into(),
            span: TextRange::new(span, span + 1),
        }),
        span: TextRange::new(span, span + 1),
    }
}

fn constructor_pattern(
    symbol: u32,
    arguments: Vec<psrs_hir::Pattern>,
    span: u32,
) -> psrs_hir::Pattern {
    psrs_hir::Pattern {
        kind: psrs_hir::PatternKind::Constructor {
            symbol: SymbolId::new(ModuleId(0), symbol),
            name_span: TextRange::new(span, span + 1),
            arguments,
        },
        span: TextRange::new(span, span + 1),
    }
}

#[test]
fn types_a_case_with_constructor_patterns() {
    let binder = LocalBinder {
        id: LocalId(0),
        name: "m".into(),
        span: TextRange::new(40, 41),
    };
    let case = expr(
        HirExprKind::Case {
            scrutinee: Box::new(local(0, 48)),
            branches: vec![
                psrs_hir::CaseBranch {
                    pattern: constructor_pattern(1, Vec::new(), 55),
                    value: integer("0", 60),
                    span: TextRange::new(55, 61),
                },
                psrs_hir::CaseBranch {
                    pattern: constructor_pattern(2, vec![var_pattern(1, "x", 70)], 70),
                    value: local(1, 74),
                    span: TextRange::new(70, 75),
                },
            ],
        },
        45,
        75,
    );
    let value = expr(
        HirExprKind::Lambda {
            binder: binder.clone(),
            body: Box::new(case),
        },
        39,
        75,
    );
    let signature = HirType {
        kind: HirTypeKind::Function {
            parameter: Box::new(applied(
                named(0, 20),
                builtin(psrs_hir::BuiltinType::Int, 26),
                20,
                30,
            )),
            result: Box::new(builtin(psrs_hir::BuiltinType::Int, 34)),
        },
        span: TextRange::new(20, 36),
    };
    let declaration = declaration_with_signature(0, "f", 19, signature, value);
    let mut resolved = module(vec![declaration], false);
    resolved.types = vec![maybe_declaration()];

    let typed = typecheck_module(resolved).unwrap();
    typed.verify().unwrap();
}

#[test]
fn rejects_a_constructor_pattern_with_the_wrong_arity() {
    let binder = LocalBinder {
        id: LocalId(0),
        name: "m".into(),
        span: TextRange::new(40, 41),
    };
    let case = expr(
        HirExprKind::Case {
            scrutinee: Box::new(local(0, 48)),
            branches: vec![psrs_hir::CaseBranch {
                pattern: constructor_pattern(2, Vec::new(), 55),
                value: integer("0", 60),
                span: TextRange::new(55, 61),
            }],
        },
        45,
        61,
    );
    let value = expr(
        HirExprKind::Lambda {
            binder: binder.clone(),
            body: Box::new(case),
        },
        39,
        61,
    );
    let signature = HirType {
        kind: HirTypeKind::Function {
            parameter: Box::new(applied(
                named(0, 20),
                builtin(psrs_hir::BuiltinType::Int, 26),
                20,
                30,
            )),
            result: Box::new(builtin(psrs_hir::BuiltinType::Int, 34)),
        },
        span: TextRange::new(20, 36),
    };
    let declaration = declaration_with_signature(0, "f", 19, signature, value);
    let mut resolved = module(vec![declaration], false);
    resolved.types = vec![maybe_declaration()];

    let errors = typecheck_module(resolved).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message().contains("expects 1 arguments")),
        "{errors:?}"
    );
}

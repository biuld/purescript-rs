#[path = "record_modes.rs"]
mod record_modes;
#[path = "wildcard.rs"]
mod wildcard;

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
                    coverage: psrs_hir::CaseBranchCoverage::Source,
                    pattern: constructor_pattern(1, Vec::new(), 55),
                    value: integer("0", 60),
                    span: TextRange::new(55, 61),
                },
                psrs_hir::CaseBranch {
                    coverage: psrs_hir::CaseBranchCoverage::Source,
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
                coverage: psrs_hir::CaseBranchCoverage::Source,
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

fn scalar_pattern(kind: psrs_hir::PatternKind, span: u32) -> psrs_hir::Pattern {
    psrs_hir::Pattern {
        kind,
        span: TextRange::new(span, span + 1),
    }
}

fn binder(id: u32, name: &str, span: u32) -> LocalBinder {
    LocalBinder {
        id: LocalId(id),
        name: name.into(),
        span: TextRange::new(span, span + name.len() as u32),
    }
}

#[test]
fn checks_integer_array_patterns_and_keeps_them_in_thir() {
    let scrutinee = binder(0, "values", 40);
    let pattern = scalar_pattern(
        psrs_hir::PatternKind::Array(vec![
            scalar_pattern(psrs_hir::PatternKind::Integer("1".into()), 59),
            scalar_pattern(psrs_hir::PatternKind::Integer("2".into()), 62),
        ]),
        58,
    );
    let value = expr(
        HirExprKind::Lambda {
            binder: scrutinee.clone(),
            body: Box::new(expr(
                HirExprKind::Case {
                    scrutinee: Box::new(local(0, 49)),
                    branches: vec![
                        psrs_hir::CaseBranch {
                            pattern,
                            value: integer("1", 70),
                            span: TextRange::new(58, 71),
                            coverage: psrs_hir::CaseBranchCoverage::Source,
                        },
                        psrs_hir::CaseBranch {
                            pattern: scalar_pattern(psrs_hir::PatternKind::Wildcard, 74),
                            value: integer("0", 79),
                            span: TextRange::new(74, 80),
                            coverage: psrs_hir::CaseBranchCoverage::Source,
                        },
                    ],
                },
                45,
                80,
            )),
        },
        39,
        80,
    );
    let typed = typecheck_module(module(vec![declaration(0, "lengthTwo", 19, value)], false))
        .expect("Array and integer patterns should typecheck together");
    typed.verify().unwrap();
    let psrs_thir::ExprKind::Lambda { body, .. } = &typed.declarations[0].value.kind else {
        panic!("expected a lambda");
    };
    let psrs_thir::ExprKind::Case { branches, .. } = &body.kind else {
        panic!("expected the pattern case");
    };
    assert!(matches!(
        branches[0].pattern.kind,
        psrs_thir::PatternKind::Array { .. }
    ));
}

#[test]
fn consumes_a_typed_pattern_after_rank_n_checking() {
    let variable = |name: &str, start| HirType {
        kind: HirTypeKind::Variable(name.into()),
        span: TextRange::new(start, start + name.len() as u32),
    };
    let arrow = |argument: HirType, result: HirType, start, end| HirType {
        kind: HirTypeKind::Function {
            parameter: Box::new(argument),
            result: Box::new(result),
        },
        span: TextRange::new(start, end),
    };
    let forall = |name: &str, ty: HirType, start| {
        let end = ty.span.end;
        HirType {
            kind: HirTypeKind::Forall {
                variables: vec![psrs_hir::TypeParameter {
                    name: name.into(),
                    name_span: TextRange::new(start, start + name.len() as u32),
                    kind: None,
                }],
                body: Box::new(ty),
            },
            span: TextRange::new(start, end),
        }
    };
    let polymorphic_identity = forall("a", arrow(variable("a", 75), variable("a", 80), 75, 81), 68);
    let signature = forall(
        "a",
        arrow(polymorphic_identity.clone(), polymorphic_identity, 50, 88),
        43,
    );
    let value = expr(
        HirExprKind::Lambda {
            binder: binder(0, "function", 40),
            body: Box::new(expr(
                HirExprKind::Case {
                    scrutinee: Box::new(local(0, 100)),
                    branches: vec![psrs_hir::CaseBranch {
                        pattern: scalar_pattern(
                            psrs_hir::PatternKind::Typed {
                                pattern: Box::new(var_pattern(1, "value", 110)),
                                ty: forall(
                                    "b",
                                    arrow(variable("b", 126), variable("b", 130), 126, 131),
                                    117,
                                ),
                            },
                            109,
                        ),
                        value: local(1, 143),
                        span: TextRange::new(109, 144),
                        coverage: psrs_hir::CaseBranchCoverage::Source,
                    }],
                },
                96,
                144,
            )),
        },
        39,
        144,
    );
    let typed = typecheck_module(module(
        vec![declaration_with_signature(0, "keep", 19, signature, value)],
        false,
    ))
    .expect("Typed binders preserve their rank-N annotation while checking");
    typed.verify().unwrap();
    let psrs_thir::ExprKind::Lambda { body, .. } = &typed.declarations[0].value.kind else {
        panic!("expected the declaration lambda");
    };
    let psrs_thir::ExprKind::Case { branches, .. } = &body.kind else {
        panic!("expected the pattern case");
    };
    assert!(matches!(
        branches[0].pattern.kind,
        psrs_thir::PatternKind::Var { .. }
    ));
}

#[test]
fn rejects_a_typed_pattern_that_disagrees_with_a_monomorphic_scrutinee() {
    let pattern = scalar_pattern(
        psrs_hir::PatternKind::Typed {
            pattern: Box::new(var_pattern(0, "value", 47)),
            ty: builtin(psrs_hir::BuiltinType::Boolean, 40),
        },
        40,
    );
    let value = expr(
        HirExprKind::Case {
            scrutinee: Box::new(integer("1", 34)),
            branches: vec![psrs_hir::CaseBranch {
                pattern,
                value: integer("0", 52),
                span: TextRange::new(40, 53),
                coverage: psrs_hir::CaseBranchCoverage::Source,
            }],
        },
        29,
        53,
    );
    let errors = typecheck_module(module(vec![declaration(0, "bad", 19, value)], false))
        .expect_err("A Boolean annotation cannot hide an Int scrutinee");
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::TypeMismatch),
        "{errors:?}"
    );
}

fn function_type(parameter: HirType, result: HirType, start: u32, end: u32) -> HirType {
    HirType {
        kind: HirTypeKind::Function {
            parameter: Box::new(parameter),
            result: Box::new(result),
        },
        span: TextRange::new(start, end),
    }
}

#[test]
fn reuses_signature_type_variables_in_pattern_and_expression_annotations() {
    let identity_signature = || {
        forall(
            vec![("a", 20)],
            function_type(variable("a", 28), variable("a", 32), 28, 33),
            19,
        )
    };
    let patterned = expr(
        HirExprKind::Lambda {
            binder: binder(0, "value", 45),
            body: Box::new(expr(
                HirExprKind::Case {
                    scrutinee: Box::new(local(0, 52)),
                    branches: vec![psrs_hir::CaseBranch {
                        pattern: scalar_pattern(
                            psrs_hir::PatternKind::Typed {
                                pattern: Box::new(var_pattern(1, "bound", 63)),
                                ty: variable("a", 58),
                            },
                            58,
                        ),
                        value: local(1, 72),
                        span: TextRange::new(58, 73),
                        coverage: psrs_hir::CaseBranchCoverage::Source,
                    }],
                },
                49,
                73,
            )),
        },
        44,
        73,
    );
    let ascribed = expr(
        HirExprKind::Lambda {
            binder: binder(2, "value", 85),
            body: Box::new(expr(
                HirExprKind::Typed {
                    expression: Box::new(local(2, 94)),
                    ty: variable("a", 100),
                },
                94,
                101,
            )),
        },
        84,
        101,
    );
    let resolved = module(
        vec![
            declaration_with_signature(0, "patterned", 19, identity_signature(), patterned),
            declaration_with_signature(1, "ascribed", 80, identity_signature(), ascribed),
        ],
        false,
    );
    typecheck_module(resolved)
        .expect("both annotations must refer to the declaration's forall variable");
}

#[test]
fn monotype_pattern_instantiates_a_rank_n_local_scrutinee() {
    let polymorphic_identity = forall(
        vec![("b", 31)],
        function_type(variable("b", 38), variable("b", 42), 38, 43),
        30,
    );
    let signature = function_type(
        polymorphic_identity,
        builtin(psrs_hir::BuiltinType::Int, 51),
        28,
        54,
    );
    let pattern = scalar_pattern(
        psrs_hir::PatternKind::Typed {
            pattern: Box::new(var_pattern(1, "function", 75)),
            ty: function_type(
                builtin(psrs_hir::BuiltinType::Int, 85),
                builtin(psrs_hir::BuiltinType::Int, 91),
                85,
                95,
            ),
        },
        74,
    );
    let call = expr(
        HirExprKind::Application(Box::new(local(1, 105)), Box::new(integer("1", 114))),
        105,
        115,
    );
    let value = expr(
        HirExprKind::Lambda {
            binder: binder(0, "polymorphic", 60),
            body: Box::new(expr(
                HirExprKind::Case {
                    scrutinee: Box::new(local(0, 68)),
                    branches: vec![psrs_hir::CaseBranch {
                        pattern,
                        value: call,
                        span: TextRange::new(74, 115),
                        coverage: psrs_hir::CaseBranchCoverage::Source,
                    }],
                },
                67,
                115,
            )),
        },
        59,
        115,
    );
    let declaration = declaration_with_signature(0, "apply", 19, signature, value);
    typecheck_module(module(vec![declaration], false))
        .expect("a monotype typed binder instantiates a rank-N scrutinee");
}

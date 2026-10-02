use super::*;

#[test]
fn infers_an_open_record_row_from_a_partial_record_pattern() {
    let value = expr(
        HirExprKind::Lambda {
            binder: binder(0, "record", 40),
            body: Box::new(expr(
                HirExprKind::Case {
                    scrutinee: Box::new(local(0, 49)),
                    branches: vec![psrs_hir::CaseBranch {
                        pattern: scalar_pattern(
                            psrs_hir::PatternKind::Record {
                                fields: vec![("selected".into(), var_pattern(1, "selected", 62))],
                                mode: psrs_hir::RecordPatternMode::Partial,
                            },
                            61,
                        ),
                        value: local(1, 77),
                        span: TextRange::new(61, 78),
                        coverage: psrs_hir::CaseBranchCoverage::Source,
                    }],
                },
                45,
                78,
            )),
        },
        39,
        78,
    );
    let typed = typecheck_module(module(vec![declaration(0, "select", 19, value)], false))
        .expect("A partial record pattern leaves the row open");
    typed.verify().unwrap();
    let Some((parameter, _)) = psrs_thir::arrow_parts(&typed.types, typed.declarations[0].ty)
    else {
        panic!("select should have an arrow type");
    };
    let row = psrs_thir::record_row(&typed.types, parameter).expect("record parameter");
    assert!(
        psrs_thir::row_fields(&typed.types, row)
            .unwrap()
            .1
            .is_some()
    );
}

#[test]
fn infers_a_closed_record_row_from_an_exact_product_pattern() {
    let value = expr(
        HirExprKind::Lambda {
            binder: binder(0, "product", 40),
            body: Box::new(expr(
                HirExprKind::Case {
                    scrutinee: Box::new(local(0, 49)),
                    branches: vec![psrs_hir::CaseBranch {
                        pattern: scalar_pattern(
                            psrs_hir::PatternKind::Record {
                                fields: vec![("_1".into(), var_pattern(1, "selected", 62))],
                                mode: psrs_hir::RecordPatternMode::Exact,
                            },
                            61,
                        ),
                        value: local(1, 77),
                        span: TextRange::new(61, 78),
                        coverage: psrs_hir::CaseBranchCoverage::Generated,
                    }],
                },
                45,
                78,
            )),
        },
        39,
        78,
    );
    let typed = typecheck_module(module(vec![declaration(0, "project", 19, value)], false))
        .expect("an exact product pattern closes the generated row");
    typed.verify().unwrap();
    let Some((parameter, _)) = psrs_thir::arrow_parts(&typed.types, typed.declarations[0].ty)
    else {
        panic!("project should have an arrow type");
    };
    let row = psrs_thir::record_row(&typed.types, parameter).expect("record parameter");
    let (fields, tail) = psrs_thir::row_fields(&typed.types, row).unwrap();
    assert_eq!(
        fields
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["_1"]
    );
    assert_eq!(tail, None, "exact product patterns require a closed row");
}

#[test]
fn partial_record_patterns_accept_scrutinees_with_additional_fields() {
    let scrutinee = expr(
        HirExprKind::Record(vec![
            ("selected".into(), integer("1", 45)),
            ("extra".into(), integer("2", 56)),
        ]),
        44,
        59,
    );
    let value = expr(
        HirExprKind::Case {
            scrutinee: Box::new(scrutinee),
            branches: vec![psrs_hir::CaseBranch {
                pattern: scalar_pattern(
                    psrs_hir::PatternKind::Record {
                        fields: vec![("selected".into(), var_pattern(0, "selected", 68))],
                        mode: psrs_hir::RecordPatternMode::Partial,
                    },
                    67,
                ),
                value: local(0, 82),
                span: TextRange::new(67, 83),
                coverage: psrs_hir::CaseBranchCoverage::Source,
            }],
        },
        39,
        83,
    );
    typecheck_module(module(vec![declaration(0, "select", 19, value)], false))
        .expect("partial record patterns accept extra fields");
}

#[test]
fn exact_product_patterns_reject_additional_known_fields() {
    let scrutinee = expr(
        HirExprKind::Record(vec![
            ("_1".into(), integer("1", 45)),
            ("_2".into(), integer("2", 53)),
        ]),
        44,
        57,
    );
    let value = expr(
        HirExprKind::Case {
            scrutinee: Box::new(scrutinee),
            branches: vec![psrs_hir::CaseBranch {
                pattern: scalar_pattern(
                    psrs_hir::PatternKind::Record {
                        fields: vec![("_1".into(), var_pattern(0, "selected", 66))],
                        mode: psrs_hir::RecordPatternMode::Exact,
                    },
                    65,
                ),
                value: local(0, 80),
                span: TextRange::new(65, 81),
                coverage: psrs_hir::CaseBranchCoverage::Generated,
            }],
        },
        39,
        81,
    );
    let errors =
        typecheck_module(module(vec![declaration(0, "project", 19, value)], false)).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.kind == TypeCheckErrorKind::TypeMismatch
            && error
                .message()
                .contains("exact record pattern omits fields _2")
    }));
}

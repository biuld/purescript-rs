use super::*;

#[test]
fn infers_an_independent_type_variable_for_a_typed_pattern_wildcard() {
    let array_type = HirType {
        kind: HirTypeKind::Application(
            Box::new(builtin(psrs_hir::BuiltinType::Array, 42)),
            Box::new(HirType {
                kind: HirTypeKind::Wildcard,
                span: TextRange::new(50, 51),
            }),
        ),
        span: TextRange::new(42, 51),
    };
    let pattern = scalar_pattern(
        psrs_hir::PatternKind::Typed {
            pattern: Box::new(var_pattern(0, "values", 38)),
            ty: array_type,
        },
        38,
    );
    let value = expr(
        HirExprKind::Case {
            scrutinee: Box::new(expr(HirExprKind::Array(vec![integer("1", 61)]), 60, 63)),
            branches: vec![psrs_hir::CaseBranch {
                pattern,
                value: local(0, 70),
                span: TextRange::new(38, 71),
                coverage: psrs_hir::CaseBranchCoverage::Source,
            }],
        },
        34,
        71,
    );
    let typed = typecheck_module(module(vec![declaration(0, "wildcard", 19, value)], false))
        .expect("a type wildcard should infer from the checked pattern column");
    typed.verify().unwrap();
}

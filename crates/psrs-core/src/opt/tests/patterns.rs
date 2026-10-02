use super::*;
use crate::{CaseBranch, Literal, Pattern, PatternKind};

const SPAN: TextRange = TextRange::new(0, 1);

fn nested_record_case(second: i32) -> Module {
    let int = TypeId(0);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Array),
        Type::Application(TypeId(1), int),
        Type::RowEmpty,
        Type::RowExtend {
            label: "items".into(),
            ty: TypeId(2),
            tail: TypeId(3),
        },
        Type::Constructor(TypeConstructor::Record),
        Type::Application(TypeId(5), TypeId(4)),
    ];
    let record = TypeId(6);
    let array = |values: Vec<i32>| Expr {
        kind: ExprKind::Array {
            elements: values
                .into_iter()
                .map(|value| Expr {
                    kind: ExprKind::Integer(value),
                    ty: int,
                    span: SPAN,
                })
                .collect(),
        },
        ty: TypeId(2),
        span: SPAN,
    };
    let scrutinee = Expr {
        kind: ExprKind::Record {
            fields: vec![("items".into(), array(vec![1, 2]))],
        },
        ty: record,
        span: SPAN,
    };
    let nested = Pattern {
        kind: PatternKind::Record {
            fields: vec![(
                "items".into(),
                Pattern {
                    kind: PatternKind::Array {
                        elements: vec![1, second]
                            .into_iter()
                            .map(|value| Pattern {
                                kind: PatternKind::Literal {
                                    value: Literal::Integer(value),
                                },
                                ty: int,
                                span: SPAN,
                            })
                            .collect(),
                    },
                    ty: TypeId(2),
                    span: SPAN,
                },
            )],
        },
        ty: record,
        span: SPAN,
    };
    let branches = vec![
        CaseBranch {
            pattern: Pattern {
                kind: PatternKind::Named {
                    id: LocalId(9),
                    pattern: Box::new(nested),
                },
                ty: record,
                span: SPAN,
            },
            value: Expr {
                kind: ExprKind::Integer(42),
                ty: int,
                span: SPAN,
            },
            span: SPAN,
            coverage: psrs_hir::CaseBranchCoverage::Source,
        },
        CaseBranch {
            pattern: Pattern {
                kind: PatternKind::Wildcard,
                ty: record,
                span: SPAN,
            },
            value: Expr {
                kind: ExprKind::Integer(20),
                ty: int,
                span: SPAN,
            },
            span: SPAN,
            coverage: psrs_hir::CaseBranchCoverage::Source,
        },
    ];
    let value = Expr {
        kind: ExprKind::Case {
            scrutinee: Box::new(scrutinee),
            branches,
        },
        ty: int,
        span: SPAN,
    };
    Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "PatternOptimization".into(),
        externals: Vec::new(),
        types: std::mem::take(&mut types),
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "select".into(),
            name_span: SPAN,
            quantified: Vec::new(),
            ty: int,
            value,
            span: SPAN,
        }],
        entry: None,
        span: SPAN,
    }
}

#[test]
fn optimizer_keeps_named_nested_array_literal_tests_in_first_match_order() {
    for nested_second in [2, 9] {
        let optimized = optimize(nested_record_case(nested_second), Budget::default())
            .expect("the nested named pattern remains well scoped");
        let ExprKind::Case { branches, .. } = &optimized.declarations[0].value.kind else {
            panic!("a named nested array test must remain a runtime decision");
        };
        assert_eq!(branches.len(), 2);
        assert!(matches!(
            &branches[0].pattern.kind,
            PatternKind::Named { pattern, .. }
                if matches!(&pattern.kind, PatternKind::Record { .. })
        ));
        assert!(matches!(branches[0].value.kind, ExprKind::Integer(42)));
        assert!(matches!(branches[1].value.kind, ExprKind::Integer(20)));
    }
}

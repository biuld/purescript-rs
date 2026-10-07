use super::*;
use psrs_hir::{Declaration, Expr, ExprKind, ExternalKind, Intrinsic, ModuleId, SymbolId};
use psrs_span::TextRange;

#[test]
fn lowers_operator_to_applications_and_preserves_source_ranges() {
    let module_id = ModuleId(0);
    let operator_id = Intrinsic::IntAdd.symbol();
    let module = hir::Module {
        id: module_id,
        name: "Main".into(),
        externals: vec![hir::ExternalSymbol {
            symbol: operator_id,
            name: "+".into(),
            kind: ExternalKind::Intrinsic(Intrinsic::IntAdd),
            signature: None,
        }],
        imports: Vec::new(),
        exports: None,
        types: Vec::new(),
        instances: Vec::new(),
        fixities: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(module_id, 0),
            name: "main".into(),
            name_span: TextRange::new(0, 4),
            value: Expr {
                kind: ExprKind::Operator {
                    operator: operator_id,
                    operator_span: TextRange::new(12, 13),
                    left: Box::new(Expr {
                        kind: ExprKind::Integer("40".into()),
                        span: TextRange::new(10, 12),
                    }),
                    right: Box::new(Expr {
                        kind: ExprKind::Integer("2".into()),
                        span: TextRange::new(14, 15),
                    }),
                },
                span: TextRange::new(10, 15),
            },
            signature: None,
            span: TextRange::new(0, 15),
        }],
        span: TextRange::new(0, 15),
    };

    let lowered = desugar_module(module).unwrap();
    let ExprKind::Application(partial, right) = &lowered.declarations[0].value.kind else {
        panic!("expected nested applications");
    };
    let ExprKind::Application(function, left) = &partial.kind else {
        panic!("expected operator application");
    };
    assert!(matches!(function.kind, ExprKind::Global(id) if id == operator_id));
    assert_eq!(function.span, TextRange::new(12, 13));
    assert_eq!(left.span, TextRange::new(10, 12));
    assert_eq!(right.span, TextRange::new(14, 15));
    assert_eq!(lowered.declarations[0].value.span, TextRange::new(10, 15));
    lowered.verify_normalized().unwrap();
}

#[test]
fn lowers_resolved_unary_minus_to_an_ordinary_function_application() {
    let module_id = ModuleId(0);
    let negate = SymbolId::new(module_id, 0);
    let main = SymbolId::new(module_id, 1);
    let module = hir::Module {
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        imports: Vec::new(),
        exports: None,
        types: Vec::new(),
        instances: Vec::new(),
        fixities: Vec::new(),
        declarations: vec![
            Declaration {
                symbol: negate,
                name: "negate".into(),
                name_span: TextRange::new(0, 6),
                value: Expr {
                    kind: ExprKind::Integer("0".into()),
                    span: TextRange::new(12, 13),
                },
                signature: None,
                span: TextRange::new(0, 13),
            },
            Declaration {
                symbol: main,
                name: "value".into(),
                name_span: TextRange::new(14, 19),
                value: Expr {
                    kind: ExprKind::Negate {
                        function: Box::new(Expr {
                            kind: ExprKind::Global(negate),
                            span: TextRange::new(22, 23),
                        }),
                        minus_span: TextRange::new(22, 23),
                        expression: Box::new(Expr {
                            kind: ExprKind::Integer("1".into()),
                            span: TextRange::new(23, 24),
                        }),
                    },
                    span: TextRange::new(22, 24),
                },
                signature: None,
                span: TextRange::new(14, 24),
            },
        ],
        span: TextRange::new(0, 24),
    };

    let lowered = desugar_module(module).unwrap();
    let value = &lowered.declarations[1].value;
    let ExprKind::Application(function, argument) = &value.kind else {
        panic!("expected a normal function application");
    };
    assert!(matches!(function.kind, ExprKind::Global(symbol) if symbol == negate));
    assert_eq!(function.span, TextRange::new(22, 23));
    assert_eq!(argument.span, TextRange::new(23, 24));
    assert_eq!(value.span, TextRange::new(22, 24));
    lowered.verify_normalized().unwrap();
}

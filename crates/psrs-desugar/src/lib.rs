use psrs_hir::{self as hir, Expr, ExprKind};

/// Lowers resolved operator nodes to ordinary symbol applications in HIR.
pub fn desugar_module(module: hir::Module) -> Result<hir::Module, Vec<hir::VerifyError>> {
    module.verify()?;
    let declarations = module
        .declarations
        .into_iter()
        .map(|mut declaration| {
            declaration.value = desugar_expr(declaration.value);
            declaration
        })
        .collect();
    let lowered = hir::Module {
        declarations,
        ..module
    };
    lowered.verify()?;
    Ok(lowered)
}

fn desugar_expr(expression: Expr) -> Expr {
    let span = expression.span;
    let kind = match expression.kind {
        ExprKind::Operator {
            operator,
            operator_span,
            left,
            right,
        } => {
            let function = Expr {
                kind: ExprKind::Global(operator),
                span: operator_span,
            };
            let left = desugar_expr(*left);
            let right = desugar_expr(*right);
            let partial_application = Expr {
                kind: ExprKind::Application(Box::new(function), Box::new(left)),
                span,
            };
            ExprKind::Application(Box::new(partial_application), Box::new(right))
        }
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(desugar_expr(*function)),
            Box::new(desugar_expr(*argument)),
        ),
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder,
            body: Box::new(desugar_expr(*body)),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|binding| hir::LocalBinding {
                    value: desugar_expr(binding.value),
                    ..binding
                })
                .collect(),
            body: Box::new(desugar_expr(*body)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(desugar_expr(*condition)),
            then_branch: Box::new(desugar_expr(*then_branch)),
            else_branch: Box::new(desugar_expr(*else_branch)),
        },
        leaf => leaf,
    };
    Expr { kind, span }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_hir::{Declaration, Intrinsic, ModuleId, SymbolId};
    use psrs_span::TextRange;

    #[test]
    fn lowers_operator_to_applications_and_preserves_source_ranges() {
        let module_id = ModuleId(0);
        let operator_id = Intrinsic::I32Add.symbol();
        let module = hir::Module {
            id: module_id,
            name: "Main".into(),
            externals: vec![hir::ExternalSymbol {
                symbol: operator_id,
                name: "+".into(),
                intrinsic: Intrinsic::I32Add,
            }],
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
        lowered.verify().unwrap();
    }
}

use psrs_hir::{self as hir, Expr, ExprKind};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesugarError {
    pub span: TextRange,
    pub message: &'static str,
}

impl From<hir::VerifyError> for DesugarError {
    fn from(error: hir::VerifyError) -> Self {
        Self {
            span: error.span,
            message: error.message,
        }
    }
}

mod fixity;
mod types;

/// Lowers resolved operator nodes to ordinary symbol applications in HIR.
pub fn desugar_module(module: hir::Module) -> Result<hir::Module, Vec<DesugarError>> {
    module.verify().map_err(|errors| {
        errors
            .into_iter()
            .map(DesugarError::from)
            .collect::<Vec<_>>()
    })?;
    let fixity_errors = fixity::validate_module(&module);
    if !fixity_errors.is_empty() {
        return Err(fixity_errors);
    }
    let declarations = module
        .declarations
        .into_iter()
        .map(|mut declaration| {
            declaration.value = desugar_expr(declaration.value);
            declaration
        })
        .collect();
    let instances = module
        .instances
        .into_iter()
        .map(|instance| hir::InstanceDeclaration {
            members: instance
                .members
                .into_iter()
                .map(|member| hir::InstanceMember {
                    value: desugar_expr(member.value),
                    ..member
                })
                .collect(),
            ..instance
        })
        .collect();
    let mut lowered = hir::Module {
        declarations,
        instances,
        ..module
    };
    types::desugar_module_types(&mut lowered);
    lowered.verify().map_err(|errors| {
        errors
            .into_iter()
            .map(DesugarError::from)
            .collect::<Vec<_>>()
    })?;
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
        ExprKind::Negate {
            function,
            expression,
            ..
        } => ExprKind::Application(
            Box::new(desugar_expr(*function)),
            Box::new(desugar_expr(*expression)),
        ),
        ExprKind::OperatorChain {
            operands,
            operators,
        } => {
            let operands = operands.into_iter().map(desugar_expr).collect();
            return desugar_expr(fixity::reassociate(operands, operators, span));
        }
        ExprKind::OperatorSection {
            operator,
            operand,
            binder,
            side,
        } => {
            let operand = desugar_expr(*operand);
            let argument = Expr {
                kind: ExprKind::Local(binder.id),
                span: binder.span,
            };
            let function = Expr {
                kind: ExprKind::Global(operator.symbol),
                span: operator.operator_span,
            };
            let (left, right) = match side {
                hir::SectionSide::Left => (operand, argument),
                hir::SectionSide::Right => (argument, operand),
            };
            let partial = Expr {
                kind: ExprKind::Application(Box::new(function), Box::new(left)),
                span,
            };
            let body = Expr {
                kind: ExprKind::Application(Box::new(partial), Box::new(right)),
                span,
            };
            ExprKind::Lambda {
                binder,
                body: Box::new(body),
            }
        }
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(desugar_expr(*function)),
            Box::new(desugar_expr(*argument)),
        ),
        ExprKind::Record(fields) => ExprKind::Record(
            fields
                .into_iter()
                .map(|(label, value)| (label, desugar_expr(value)))
                .collect(),
        ),
        ExprKind::RecordUpdate { expression, fields } => ExprKind::RecordUpdate {
            expression: Box::new(desugar_expr(*expression)),
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, desugar_expr(value)))
                .collect(),
        },
        ExprKind::FieldAccess { expression, field } => ExprKind::FieldAccess {
            expression: Box::new(desugar_expr(*expression)),
            field,
        },
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
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(desugar_expr(*scrutinee)),
            branches: branches
                .into_iter()
                .map(|branch| hir::CaseBranch {
                    pattern: desugar_pattern(branch.pattern),
                    value: desugar_expr(branch.value),
                    span: branch.span,
                })
                .collect(),
        },
        leaf => leaf,
    };
    Expr { kind, span }
}

fn desugar_pattern(pattern: hir::Pattern) -> hir::Pattern {
    let span = pattern.span;
    let kind = match pattern.kind {
        hir::PatternKind::Constructor {
            symbol,
            name_span,
            arguments,
        } => hir::PatternKind::Constructor {
            symbol,
            name_span,
            arguments: arguments.into_iter().map(desugar_pattern).collect(),
        },
        hir::PatternKind::OperatorChain {
            operands,
            operators,
        } => {
            return fixity::reassociate_pattern(
                operands.into_iter().map(desugar_pattern).collect(),
                operators,
                span,
            );
        }
        hir::PatternKind::Record { fields } => hir::PatternKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, pattern)| (label, desugar_pattern(pattern)))
                .collect(),
        },
        leaf => leaf,
    };
    hir::Pattern { kind, span }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_hir::{Declaration, ExternalKind, Intrinsic, ModuleId, SymbolId};
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
                kind: ExternalKind::Intrinsic(Intrinsic::I32Add),
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
        lowered.verify().unwrap();
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
        lowered.verify().unwrap();
    }
}

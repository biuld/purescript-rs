use psrs_hir::{self as hir, Expr, ExprKind, Guard};
use psrs_span::TextRange;
use std::collections::HashSet;

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

mod alpha;
mod boolean_case;
mod boolean_product_case;
mod case_helpers;
mod constant_truth;
mod expr;
mod fixity;
mod free_vars;
mod guards;
mod types;

/// Lowers resolved operators, equations, and guards to ordinary HIR.
pub fn desugar_module(module: hir::Module) -> Result<hir::Module, Vec<DesugarError>> {
    let true_symbols = true_symbols(std::slice::from_ref(&module));
    desugar_module_with_true_symbols(module, &true_symbols)
}

/// Finds declarations proven to be transparent aliases of Boolean `true`.
pub fn true_symbols(modules: &[hir::Module]) -> HashSet<hir::SymbolId> {
    constant_truth::true_symbols(modules)
}

/// Lowers one module using proofs computed from the complete resolved program.
pub fn desugar_module_with_true_symbols(
    module: hir::Module,
    true_symbols: &HashSet<hir::SymbolId>,
) -> Result<hir::Module, Vec<DesugarError>> {
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
    let mut module = hir::Module {
        declarations: module
            .declarations
            .into_iter()
            .map(|mut declaration| {
                declaration.value = desugar_expr(declaration.value);
                declaration
            })
            .collect(),
        instances: module
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
            .collect(),
        ..module
    };
    types::desugar_module_types(&mut module);
    module.verify().map_err(|errors| {
        errors
            .into_iter()
            .map(DesugarError::from)
            .collect::<Vec<_>>()
    })?;
    let mut desugarer = expr::Desugarer::new(&module, true_symbols);
    for declaration in &mut module.declarations {
        declaration.value = desugarer.lower(declaration.value.clone());
    }
    for instance in &mut module.instances {
        for member in &mut instance.members {
            member.value = desugarer.lower(member.value.clone());
        }
    }
    if !desugarer.errors.is_empty() {
        return Err(desugarer
            .errors
            .into_iter()
            .map(DesugarError::from)
            .collect());
    }
    module.verify_normalized().map_err(|errors| {
        errors
            .into_iter()
            .map(DesugarError::from)
            .collect::<Vec<_>>()
    })?;
    Ok(module)
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
        ExprKind::Array(elements) => {
            ExprKind::Array(elements.into_iter().map(desugar_expr).collect())
        }
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
        ExprKind::Typed { expression, ty } => ExprKind::Typed {
            expression: Box::new(desugar_expr(*expression)),
            ty,
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
                    ..branch
                })
                .collect(),
        },
        ExprKind::Guarded(clauses) => ExprKind::Guarded(
            clauses
                .into_iter()
                .map(|mut clause| {
                    clause.where_bindings = clause
                        .where_bindings
                        .into_iter()
                        .map(|binding| hir::LocalBinding {
                            value: desugar_expr(binding.value),
                            ..binding
                        })
                        .collect();
                    clause.guards = clause
                        .guards
                        .into_iter()
                        .map(|guard| match guard {
                            Guard::Boolean(expression) => Guard::Boolean(desugar_expr(expression)),
                            Guard::Pattern { pattern, value } => Guard::Pattern {
                                pattern: desugar_pattern(pattern),
                                value: desugar_expr(value),
                            },
                            Guard::Let { bindings, span } => Guard::Let {
                                bindings: bindings
                                    .into_iter()
                                    .map(|binding| hir::LocalBinding {
                                        value: desugar_expr(binding.value),
                                        ..binding
                                    })
                                    .collect(),
                                span,
                            },
                        })
                        .collect();
                    clause.value = desugar_expr(clause.value);
                    clause
                })
                .collect(),
        ),
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
mod tests;

use super::VerifyError;
use crate::{Expr, ExprKind, Guard, Pattern, PatternKind};

pub(crate) fn normalized(module: &crate::Module) -> Result<(), Vec<VerifyError>> {
    let mut errors = Vec::new();
    for declaration in &module.declarations {
        check_normalized_expr(&declaration.value, &mut errors);
    }
    for instance in &module.instances {
        for member in &instance.members {
            check_normalized_expr(&member.value, &mut errors);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_normalized_expr(expression: &Expr, errors: &mut Vec<VerifyError>) {
    match &expression.kind {
        ExprKind::Guarded(clauses) => {
            errors.push(VerifyError {
                span: expression.span,
                message: "guarded expression survived P4 desugaring",
            });
            for clause in clauses {
                for guard in &clause.guards {
                    match guard {
                        Guard::Boolean(value) | Guard::Pattern { value, .. } => {
                            check_normalized_expr(value, errors);
                        }
                        Guard::Let { bindings, .. } => {
                            for binding in bindings {
                                check_normalized_expr(&binding.value, errors);
                            }
                        }
                    }
                    if let Guard::Pattern { pattern, .. } = guard {
                        check_normalized_pattern(pattern, errors);
                    }
                }
                for binding in &clause.where_bindings {
                    check_normalized_expr(&binding.value, errors);
                }
                check_normalized_expr(&clause.value, errors);
            }
        }
        ExprKind::Operator { left, right, .. } => {
            errors.push(VerifyError {
                span: expression.span,
                message: "operator expression survived P4 desugaring",
            });
            check_normalized_expr(left, errors);
            check_normalized_expr(right, errors);
        }
        ExprKind::OperatorChain { operands, .. } => {
            errors.push(VerifyError {
                span: expression.span,
                message: "operator chain survived P4 desugaring",
            });
            for operand in operands {
                check_normalized_expr(operand, errors);
            }
        }
        ExprKind::OperatorSection { operand, .. } => {
            errors.push(VerifyError {
                span: expression.span,
                message: "operator section survived P4 desugaring",
            });
            check_normalized_expr(operand, errors);
        }
        ExprKind::Negate {
            function,
            expression: operand,
            ..
        } => {
            errors.push(VerifyError {
                span: expression.span,
                message: "unary minus survived P4 desugaring",
            });
            check_normalized_expr(function, errors);
            check_normalized_expr(operand, errors);
        }
        ExprKind::Array(items) => {
            for item in items {
                check_normalized_expr(item, errors);
            }
        }
        ExprKind::Record(fields) | ExprKind::MatchProduct(fields) => {
            for (_, value) in fields {
                check_normalized_expr(value, errors);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            check_normalized_expr(expression, errors);
            for (_, value) in fields {
                check_normalized_expr(value, errors);
            }
        }
        ExprKind::FieldAccess { expression, .. }
        | ExprKind::Typed { expression, .. }
        | ExprKind::TypeApplication { expression, .. } => {
            check_normalized_expr(expression, errors);
        }
        ExprKind::Application(function, argument) => {
            check_normalized_expr(function, errors);
            check_normalized_expr(argument, errors);
        }
        ExprKind::Lambda { body, .. } => check_normalized_expr(body, errors),
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                check_normalized_expr(&binding.value, errors);
            }
            check_normalized_expr(body, errors);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            check_normalized_expr(condition, errors);
            check_normalized_expr(then_branch, errors);
            check_normalized_expr(else_branch, errors);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            check_normalized_expr(scrutinee, errors);
            for branch in branches {
                check_normalized_pattern(&branch.pattern, errors);
                check_normalized_expr(&branch.value, errors);
            }
        }
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}

fn check_normalized_pattern(pattern: &Pattern, errors: &mut Vec<VerifyError>) {
    match &pattern.kind {
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                check_normalized_pattern(argument, errors);
            }
        }
        PatternKind::OperatorChain { operands, .. } => {
            errors.push(VerifyError {
                span: pattern.span,
                message: "operator pattern chain survived P4 desugaring",
            });
            for operand in operands {
                check_normalized_pattern(operand, errors);
            }
        }
        PatternKind::Record { fields, .. } => {
            for (_, pattern) in fields {
                check_normalized_pattern(pattern, errors);
            }
        }
        PatternKind::Array(elements) => {
            for element in elements {
                check_normalized_pattern(element, errors);
            }
        }
        PatternKind::Named { pattern, .. } | PatternKind::Typed { pattern, .. } => {
            check_normalized_pattern(pattern, errors);
        }
        PatternKind::Wildcard
        | PatternKind::Boolean(_)
        | PatternKind::Integer(_)
        | PatternKind::Number(_)
        | PatternKind::String(_)
        | PatternKind::Char(_)
        | PatternKind::Var(_) => {}
    }
}

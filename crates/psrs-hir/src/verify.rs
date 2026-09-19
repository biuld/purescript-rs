use crate::{Expr, ExprKind, LocalId, Pattern, PatternKind, SymbolId};
use psrs_span::TextRange;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    pub span: TextRange,
    pub message: &'static str,
}

pub(crate) fn verify_expr(
    expression: &Expr,
    globals: &HashSet<SymbolId>,
    visible_locals: &mut HashSet<LocalId>,
    declared_locals: &mut HashSet<LocalId>,
    errors: &mut Vec<VerifyError>,
) {
    match &expression.kind {
        ExprKind::Local(id) if !visible_locals.contains(id) => errors.push(VerifyError {
            span: expression.span,
            message: "local reference is not in scope",
        }),
        ExprKind::Global(id) if !globals.contains(id) => errors.push(VerifyError {
            span: expression.span,
            message: "global reference does not name a module declaration",
        }),
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
        ExprKind::Array(elements) => {
            for element in elements {
                verify_expr(element, globals, visible_locals, declared_locals, errors);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                verify_expr(value, globals, visible_locals, declared_locals, errors);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            verify_expr(expression, globals, visible_locals, declared_locals, errors);
            for (_, value) in fields {
                verify_expr(value, globals, visible_locals, declared_locals, errors);
            }
        }
        ExprKind::FieldAccess { expression, .. } => {
            verify_expr(expression, globals, visible_locals, declared_locals, errors);
        }
        ExprKind::Application(function, argument) => {
            verify_expr(function, globals, visible_locals, declared_locals, errors);
            verify_expr(argument, globals, visible_locals, declared_locals, errors);
        }
        ExprKind::Operator {
            operator,
            left,
            right,
            ..
        } => {
            if !globals.contains(operator) {
                errors.push(VerifyError {
                    span: expression.span,
                    message: "operator symbol is not declared in the module or intrinsic set",
                });
            }
            verify_expr(left, globals, visible_locals, declared_locals, errors);
            verify_expr(right, globals, visible_locals, declared_locals, errors);
        }
        ExprKind::Lambda { binder, body } => {
            if !declared_locals.insert(binder.id) {
                errors.push(VerifyError {
                    span: binder.span,
                    message: "duplicate local ID",
                });
            }
            let inserted = visible_locals.insert(binder.id);
            verify_expr(body, globals, visible_locals, declared_locals, errors);
            if inserted {
                visible_locals.remove(&binder.id);
            }
        }
        ExprKind::Let { bindings, body } => {
            let mut inserted = Vec::new();
            for binding in bindings {
                if !declared_locals.insert(binding.binder.id) {
                    errors.push(VerifyError {
                        span: binding.binder.span,
                        message: "duplicate local ID",
                    });
                }
                if visible_locals.insert(binding.binder.id) {
                    inserted.push(binding.binder.id);
                }
            }
            for binding in bindings {
                verify_expr(
                    &binding.value,
                    globals,
                    visible_locals,
                    declared_locals,
                    errors,
                );
            }
            verify_expr(body, globals, visible_locals, declared_locals, errors);
            for id in inserted {
                visible_locals.remove(&id);
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            verify_expr(condition, globals, visible_locals, declared_locals, errors);
            verify_expr(
                then_branch,
                globals,
                visible_locals,
                declared_locals,
                errors,
            );
            verify_expr(
                else_branch,
                globals,
                visible_locals,
                declared_locals,
                errors,
            );
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            verify_expr(scrutinee, globals, visible_locals, declared_locals, errors);
            for branch in branches {
                let mut inserted = Vec::new();
                verify_pattern(
                    &branch.pattern,
                    globals,
                    visible_locals,
                    declared_locals,
                    &mut inserted,
                    errors,
                );
                verify_expr(
                    &branch.value,
                    globals,
                    visible_locals,
                    declared_locals,
                    errors,
                );
                for id in inserted {
                    visible_locals.remove(&id);
                }
            }
        }
    }
}

fn verify_pattern(
    pattern: &Pattern,
    globals: &HashSet<SymbolId>,
    visible_locals: &mut HashSet<LocalId>,
    declared_locals: &mut HashSet<LocalId>,
    inserted: &mut Vec<LocalId>,
    errors: &mut Vec<VerifyError>,
) {
    match &pattern.kind {
        PatternKind::Wildcard => {}
        PatternKind::Var(binder) => {
            if !declared_locals.insert(binder.id) {
                errors.push(VerifyError {
                    span: binder.span,
                    message: "duplicate local ID",
                });
            }
            if visible_locals.insert(binder.id) {
                inserted.push(binder.id);
            }
        }
        PatternKind::Constructor {
            symbol, arguments, ..
        } => {
            if !globals.contains(symbol) {
                errors.push(VerifyError {
                    span: pattern.span,
                    message: "pattern constructor is not a module declaration",
                });
            }
            for argument in arguments {
                verify_pattern(
                    argument,
                    globals,
                    visible_locals,
                    declared_locals,
                    inserted,
                    errors,
                );
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                verify_pattern(
                    field,
                    globals,
                    visible_locals,
                    declared_locals,
                    inserted,
                    errors,
                );
            }
        }
    }
}

use crate::{Expr, ExprKind, Guard, GuardedExpr, LocalId, Module, Pattern, PatternKind, SymbolId};

mod normalized;
pub(crate) use normalized::normalized;
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
        ExprKind::Typed { expression, .. } | ExprKind::TypeApplication { expression, .. } => {
            verify_expr(expression, globals, visible_locals, declared_locals, errors);
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
        ExprKind::Negate {
            function,
            expression,
            ..
        } => {
            verify_expr(function, globals, visible_locals, declared_locals, errors);
            verify_expr(expression, globals, visible_locals, declared_locals, errors);
        }
        ExprKind::OperatorChain {
            operands,
            operators,
        } => {
            if operands.len() != operators.len() + 1 {
                errors.push(VerifyError {
                    span: expression.span,
                    message: "operator chain must have exactly one more operand than operator",
                });
            }
            for operator in operators {
                if let Some(local) = operator.local {
                    if !visible_locals.contains(&local) {
                        errors.push(VerifyError {
                            span: operator.operator_span,
                            message: "backticked operator is not in scope",
                        });
                    }
                } else if !globals.contains(&operator.symbol) {
                    errors.push(VerifyError {
                        span: operator.operator_span,
                        message: "operator symbol is not declared in the module or intrinsic set",
                    });
                }
            }
            for operand in operands {
                verify_expr(operand, globals, visible_locals, declared_locals, errors);
            }
        }
        ExprKind::OperatorSection {
            operator,
            operand,
            binder,
            ..
        } => {
            if let Some(local) = operator.local {
                if !visible_locals.contains(&local) {
                    errors.push(VerifyError {
                        span: operator.operator_span,
                        message: "backticked operator is not in scope",
                    });
                }
            } else if !globals.contains(&operator.symbol) {
                errors.push(VerifyError {
                    span: operator.operator_span,
                    message: "operator symbol is not declared in the module or intrinsic set",
                });
            }
            if !declared_locals.insert(binder.id) {
                errors.push(VerifyError {
                    span: binder.span,
                    message: "duplicate local ID",
                });
            }
            verify_expr(operand, globals, visible_locals, declared_locals, errors);
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
        ExprKind::Guarded(clauses) => {
            for clause in clauses {
                verify_guarded_expr(clause, globals, visible_locals, declared_locals, errors);
            }
        }
    }
}

pub(crate) fn exported_instances(module: &Module) -> Vec<VerifyError> {
    let Some(exports) = &module.exports else {
        return Vec::new();
    };
    let declared: HashSet<SymbolId> = module
        .instances
        .iter()
        .map(|instance| instance.symbol)
        .collect();
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    for exported in &exports.instances {
        let message = if exported.symbol.module != module.id {
            Some("exported instance belongs to a different module")
        } else if !declared.contains(&exported.symbol) {
            Some("exported instance is not declared in module")
        } else if !seen.insert(exported.symbol) {
            Some("duplicate exported instance symbol")
        } else {
            None
        };
        if let Some(message) = message {
            errors.push(VerifyError {
                span: exported.name_span,
                message,
            });
        }
    }
    errors
}

fn verify_guarded_expr(
    clause: &GuardedExpr,
    globals: &HashSet<SymbolId>,
    visible_locals: &mut HashSet<LocalId>,
    declared_locals: &mut HashSet<LocalId>,
    errors: &mut Vec<VerifyError>,
) {
    let mut inserted = Vec::new();
    for binding in &clause.where_bindings {
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
    for binding in &clause.where_bindings {
        verify_expr(
            &binding.value,
            globals,
            visible_locals,
            declared_locals,
            errors,
        );
    }
    for guard in &clause.guards {
        match guard {
            Guard::Boolean(expression) => {
                verify_expr(expression, globals, visible_locals, declared_locals, errors);
            }
            Guard::Pattern { pattern, value } => {
                verify_expr(value, globals, visible_locals, declared_locals, errors);
                verify_pattern(
                    pattern,
                    globals,
                    visible_locals,
                    declared_locals,
                    &mut inserted,
                    errors,
                );
            }
            Guard::Let { bindings, .. } => {
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
            }
        }
    }
    verify_expr(
        &clause.value,
        globals,
        visible_locals,
        declared_locals,
        errors,
    );
    for id in inserted {
        visible_locals.remove(&id);
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
        PatternKind::Wildcard
        | PatternKind::Boolean(_)
        | PatternKind::Integer(_)
        | PatternKind::Number(_)
        | PatternKind::String(_)
        | PatternKind::Char(_) => {}
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
        PatternKind::OperatorChain {
            operands,
            operators,
        } => {
            if operands.len() != operators.len() + 1 {
                errors.push(VerifyError {
                    span: pattern.span,
                    message: "operator pattern chain must have one more operand than operator",
                });
            }
            for operator in operators {
                if !globals.contains(&operator.symbol) {
                    errors.push(VerifyError {
                        span: operator.operator_span,
                        message: "pattern operator is not a module constructor",
                    });
                }
            }
            for operand in operands {
                verify_pattern(
                    operand,
                    globals,
                    visible_locals,
                    declared_locals,
                    inserted,
                    errors,
                );
            }
        }
        PatternKind::Record { fields, .. } => {
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
        PatternKind::Array(elements) => {
            for element in elements {
                verify_pattern(
                    element,
                    globals,
                    visible_locals,
                    declared_locals,
                    inserted,
                    errors,
                );
            }
        }
        PatternKind::Named { binder, pattern } => {
            if !declared_locals.insert(binder.id) {
                errors.push(VerifyError {
                    span: binder.span,
                    message: "duplicate local ID",
                });
            }
            if visible_locals.insert(binder.id) {
                inserted.push(binder.id);
            }
            verify_pattern(
                pattern,
                globals,
                visible_locals,
                declared_locals,
                inserted,
                errors,
            );
        }
        PatternKind::Typed { pattern, .. } => verify_pattern(
            pattern,
            globals,
            visible_locals,
            declared_locals,
            inserted,
            errors,
        ),
    }
}

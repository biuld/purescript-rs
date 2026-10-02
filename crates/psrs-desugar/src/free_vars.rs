use psrs_hir::{CaseBranch, Expr, ExprKind, Guard, GuardedExpr, LocalId, Pattern, PatternKind};
use std::collections::{HashMap, HashSet};

pub(super) fn captures(branches: &[CaseBranch]) -> Vec<LocalId> {
    let mut free = HashSet::new();
    for branch in branches {
        let mut bound = HashSet::new();
        pattern_ids(&branch.pattern, &mut bound);
        collect(&branch.value, &mut bound, &mut free);
    }
    let mut captures = free.into_iter().collect::<Vec<_>>();
    captures.sort_by_key(|id| id.0);
    captures
}

pub(super) fn rebind(expression: Expr, mapping: &HashMap<LocalId, LocalId>) -> Expr {
    let span = expression.span;
    let kind = match expression.kind {
        ExprKind::Local(id) => ExprKind::Local(mapping.get(&id).copied().unwrap_or(id)),
        ExprKind::Array(items) => ExprKind::Array(
            items
                .into_iter()
                .map(|item| rebind(item, mapping))
                .collect(),
        ),
        ExprKind::Record(fields) => ExprKind::Record(
            fields
                .into_iter()
                .map(|(label, value)| (label, rebind(value, mapping)))
                .collect(),
        ),
        ExprKind::RecordUpdate { expression, fields } => ExprKind::RecordUpdate {
            expression: Box::new(rebind(*expression, mapping)),
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, rebind(value, mapping)))
                .collect(),
        },
        ExprKind::FieldAccess { expression, field } => ExprKind::FieldAccess {
            expression: Box::new(rebind(*expression, mapping)),
            field,
        },
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(rebind(*function, mapping)),
            Box::new(rebind(*argument, mapping)),
        ),
        ExprKind::Typed { expression, ty } => ExprKind::Typed {
            expression: Box::new(rebind(*expression, mapping)),
            ty,
        },
        ExprKind::Operator {
            operator,
            operator_span,
            left,
            right,
        } => ExprKind::Operator {
            operator,
            operator_span,
            left: Box::new(rebind(*left, mapping)),
            right: Box::new(rebind(*right, mapping)),
        },
        ExprKind::OperatorChain {
            operands,
            operators,
        } => ExprKind::OperatorChain {
            operands: operands
                .into_iter()
                .map(|operand| rebind(operand, mapping))
                .collect(),
            operators,
        },
        ExprKind::OperatorSection {
            operator,
            operand,
            binder,
            side,
        } => ExprKind::OperatorSection {
            operator,
            operand: Box::new(rebind(*operand, mapping)),
            binder,
            side,
        },
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder,
            body: Box::new(rebind(*body, mapping)),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|binding| LocalBinding {
                    value: rebind(binding.value, mapping),
                    ..binding
                })
                .collect(),
            body: Box::new(rebind(*body, mapping)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(rebind(*condition, mapping)),
            then_branch: Box::new(rebind(*then_branch, mapping)),
            else_branch: Box::new(rebind(*else_branch, mapping)),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(rebind(*scrutinee, mapping)),
            branches: branches
                .into_iter()
                .map(|branch| CaseBranch {
                    value: rebind(branch.value, mapping),
                    ..branch
                })
                .collect(),
        },
        ExprKind::Guarded(clauses) => ExprKind::Guarded(
            clauses
                .into_iter()
                .map(|clause| rebind_clause(clause, mapping))
                .collect(),
        ),
        leaf @ (ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)) => leaf,
    };
    Expr { kind, span }
}

use psrs_hir::LocalBinding;

fn rebind_clause(clause: GuardedExpr, mapping: &HashMap<LocalId, LocalId>) -> GuardedExpr {
    GuardedExpr {
        guards: clause
            .guards
            .into_iter()
            .map(|guard| match guard {
                Guard::Boolean(value) => Guard::Boolean(rebind(value, mapping)),
                Guard::Pattern { pattern, value } => Guard::Pattern {
                    pattern,
                    value: rebind(value, mapping),
                },
                Guard::Let { bindings, span } => Guard::Let {
                    bindings: bindings
                        .into_iter()
                        .map(|binding| LocalBinding {
                            value: rebind(binding.value, mapping),
                            ..binding
                        })
                        .collect(),
                    span,
                },
            })
            .collect(),
        value: rebind(clause.value, mapping),
        where_bindings: clause
            .where_bindings
            .into_iter()
            .map(|binding| LocalBinding {
                value: rebind(binding.value, mapping),
                ..binding
            })
            .collect(),
        ..clause
    }
}

fn collect(expression: &Expr, bound: &mut HashSet<LocalId>, free: &mut HashSet<LocalId>) {
    match &expression.kind {
        ExprKind::Local(id) => {
            if !bound.contains(id) {
                free.insert(*id);
            }
        }
        ExprKind::Array(items) => {
            for item in items {
                collect(item, bound, free);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                collect(value, bound, free);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            collect(expression, bound, free);
            for (_, value) in fields {
                collect(value, bound, free);
            }
        }
        ExprKind::FieldAccess { expression, .. } | ExprKind::Typed { expression, .. } => {
            collect(expression, bound, free);
        }
        ExprKind::Application(function, argument) => {
            collect(function, bound, free);
            collect(argument, bound, free);
        }
        ExprKind::Operator { left, right, .. } => {
            collect(left, bound, free);
            collect(right, bound, free);
        }
        ExprKind::OperatorChain { operands, .. } => {
            for operand in operands {
                collect(operand, bound, free);
            }
        }
        ExprKind::OperatorSection { operand, .. } => collect(operand, bound, free),
        ExprKind::Lambda { binder, body } => {
            let inserted = bound.insert(binder.id);
            collect(body, bound, free);
            if inserted {
                bound.remove(&binder.id);
            }
        }
        ExprKind::Let { bindings, body } => {
            let mut inserted = Vec::new();
            for binding in bindings {
                if bound.insert(binding.binder.id) {
                    inserted.push(binding.binder.id);
                }
            }
            for binding in bindings {
                collect(&binding.value, bound, free);
            }
            collect(body, bound, free);
            for id in inserted {
                bound.remove(&id);
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect(condition, bound, free);
            collect(then_branch, bound, free);
            collect(else_branch, bound, free);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect(scrutinee, bound, free);
            for branch in branches {
                let mut inserted = Vec::new();
                let mut ids = HashSet::new();
                pattern_ids(&branch.pattern, &mut ids);
                for id in ids {
                    if bound.insert(id) {
                        inserted.push(id);
                    }
                }
                collect(&branch.value, bound, free);
                for id in inserted {
                    bound.remove(&id);
                }
            }
        }
        ExprKind::Guarded(clauses) => {
            for clause in clauses {
                collect_clause(clause, bound, free);
            }
        }
        ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}

fn collect_clause(clause: &GuardedExpr, bound: &mut HashSet<LocalId>, free: &mut HashSet<LocalId>) {
    let mut inserted = Vec::new();
    for binding in &clause.where_bindings {
        if bound.insert(binding.binder.id) {
            inserted.push(binding.binder.id);
        }
    }
    for binding in &clause.where_bindings {
        collect(&binding.value, bound, free);
    }
    for guard in &clause.guards {
        match guard {
            Guard::Boolean(value) => collect(value, bound, free),
            Guard::Pattern { pattern, value } => {
                collect(value, bound, free);
                let mut ids = HashSet::new();
                pattern_ids(pattern, &mut ids);
                for id in ids {
                    if bound.insert(id) {
                        inserted.push(id);
                    }
                }
            }
            Guard::Let { bindings, .. } => {
                for binding in bindings {
                    if bound.insert(binding.binder.id) {
                        inserted.push(binding.binder.id);
                    }
                }
                for binding in bindings {
                    collect(&binding.value, bound, free);
                }
            }
        }
    }
    collect(&clause.value, bound, free);
    for id in inserted {
        bound.remove(&id);
    }
}

fn pattern_ids(pattern: &Pattern, ids: &mut HashSet<LocalId>) {
    match &pattern.kind {
        PatternKind::Var(binder) => {
            ids.insert(binder.id);
        }
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                pattern_ids(argument, ids);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                pattern_ids(field, ids);
            }
        }
        PatternKind::OperatorChain { operands, .. } => {
            for operand in operands {
                pattern_ids(operand, ids);
            }
        }
        PatternKind::Wildcard | PatternKind::Boolean(_) => {}
    }
}

use crate::{Binding, Expr, ExprKind, LocalId, Module, Pattern, PatternKind};
use std::collections::{HashMap, HashSet};

pub(super) use crate::locals::FreshLocals;

pub(super) fn count_nodes(expression: &Expr) -> usize {
    1 + match &expression.kind {
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => 0,
        ExprKind::Unit | ExprKind::StateToken | ExprKind::Trap => 0,
        ExprKind::Constructor { arguments, .. }
        | ExprKind::IntrinsicCall { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => arguments.iter().map(count_nodes).sum(),
        ExprKind::Record { fields } => fields.iter().map(|(_, value)| count_nodes(value)).sum(),
        ExprKind::RecordUpdate { record, fields } => {
            count_nodes(record)
                + fields
                    .iter()
                    .map(|(_, value)| count_nodes(value))
                    .sum::<usize>()
        }
        ExprKind::FieldAccess { record, .. }
        | ExprKind::RepresentationCast { value: record, .. } => count_nodes(record),
        ExprKind::Application(left, right) => count_nodes(left) + count_nodes(right),
        ExprKind::Lambda { body, .. } => count_nodes(body),
        ExprKind::Let { bindings, body } => {
            bindings
                .iter()
                .map(|binding| count_nodes(&binding.value))
                .sum::<usize>()
                + count_nodes(body)
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => count_nodes(condition) + count_nodes(then_branch) + count_nodes(else_branch),
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            count_nodes(scrutinee)
                + branches
                    .iter()
                    .map(|branch| count_nodes(&branch.value))
                    .sum::<usize>()
        }
    }
}

pub(super) fn with_span(mut expression: Expr, span: psrs_span::TextRange) -> Expr {
    expression.span = span;
    expression
}

pub(super) fn next_locals(module: &Module) -> Vec<FreshLocals> {
    module
        .declarations
        .iter()
        .map(|declaration| FreshLocals::for_declaration(&declaration.value))
        .collect()
}

pub(super) fn substitute_locals(expression: &Expr, substitutions: &HashMap<LocalId, Expr>) -> Expr {
    substitute_inner(expression, substitutions, &mut HashSet::new())
}

fn substitute_inner(
    expression: &Expr,
    substitutions: &HashMap<LocalId, Expr>,
    shadowed: &mut HashSet<LocalId>,
) -> Expr {
    if let ExprKind::Local(id) = &expression.kind
        && !shadowed.contains(id)
        && let Some(replacement) = substitutions.get(id)
    {
        return replacement.clone();
    }
    let kind = match &expression.kind {
        ExprKind::Local(id) => ExprKind::Local(*id),
        ExprKind::Global(symbol) => ExprKind::Global(*symbol),
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol: *symbol,
            arguments: arguments
                .iter()
                .map(|argument| substitute_inner(argument, substitutions, shadowed))
                .collect(),
        },
        ExprKind::IntrinsicCall {
            intrinsic,
            arguments,
        } => ExprKind::IntrinsicCall {
            intrinsic: *intrinsic,
            arguments: arguments
                .iter()
                .map(|argument| substitute_inner(argument, substitutions, shadowed))
                .collect(),
        },
        ExprKind::Integer(value) => ExprKind::Integer(*value),
        ExprKind::Number(value) => ExprKind::Number(value.clone()),
        ExprKind::Boolean(value) => ExprKind::Boolean(*value),
        ExprKind::String(value) => ExprKind::String(value.clone()),
        ExprKind::Char(value) => ExprKind::Char(*value),
        ExprKind::Unit => ExprKind::Unit,
        ExprKind::StateToken => ExprKind::StateToken,
        ExprKind::Trap => ExprKind::Trap,
        ExprKind::Array { elements } => ExprKind::Array {
            elements: elements
                .iter()
                .map(|element| substitute_inner(element, substitutions, shadowed))
                .collect(),
        },
        ExprKind::Record { fields } => ExprKind::Record {
            fields: fields
                .iter()
                .map(|(label, value)| {
                    (
                        label.clone(),
                        substitute_inner(value, substitutions, shadowed),
                    )
                })
                .collect(),
        },
        ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
            record: Box::new(substitute_inner(record, substitutions, shadowed)),
            fields: fields
                .iter()
                .map(|(label, value)| {
                    (
                        label.clone(),
                        substitute_inner(value, substitutions, shadowed),
                    )
                })
                .collect(),
        },
        ExprKind::FieldAccess { record, field } => ExprKind::FieldAccess {
            record: Box::new(substitute_inner(record, substitutions, shadowed)),
            field: field.clone(),
        },
        ExprKind::RepresentationCast {
            value,
            source_type,
            target_type,
        } => ExprKind::RepresentationCast {
            value: Box::new(substitute_inner(value, substitutions, shadowed)),
            source_type: *source_type,
            target_type: *target_type,
        },
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(substitute_inner(function, substitutions, shadowed)),
            Box::new(substitute_inner(argument, substitutions, shadowed)),
        ),
        ExprKind::Lambda { binder, body } => {
            let was_shadowed = shadowed.insert(binder.id);
            let body = substitute_inner(body, substitutions, shadowed);
            restore_shadow(shadowed, binder.id, was_shadowed);
            ExprKind::Lambda {
                binder: binder.clone(),
                body: Box::new(body),
            }
        }
        ExprKind::Let { bindings, body } => {
            let previous = bindings
                .iter()
                .map(|binding| (binding.binder.id, shadowed.insert(binding.binder.id)))
                .collect::<Vec<_>>();
            let bindings = bindings
                .iter()
                .map(|binding| Binding {
                    binder: binding.binder.clone(),
                    quantified: binding.quantified.clone(),
                    value: substitute_inner(&binding.value, substitutions, shadowed),
                    span: binding.span,
                })
                .collect();
            let body = substitute_inner(body, substitutions, shadowed);
            for (id, was_shadowed) in previous.into_iter().rev() {
                restore_shadow(shadowed, id, was_shadowed);
            }
            ExprKind::Let {
                bindings,
                body: Box::new(body),
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(substitute_inner(condition, substitutions, shadowed)),
            then_branch: Box::new(substitute_inner(then_branch, substitutions, shadowed)),
            else_branch: Box::new(substitute_inner(else_branch, substitutions, shadowed)),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(substitute_inner(scrutinee, substitutions, shadowed)),
            branches: branches
                .iter()
                .map(|branch| {
                    let ids = pattern_ids(&branch.pattern);
                    let previous = ids
                        .iter()
                        .map(|id| (*id, shadowed.insert(*id)))
                        .collect::<Vec<_>>();
                    let value = substitute_inner(&branch.value, substitutions, shadowed);
                    for (id, was_shadowed) in previous.into_iter().rev() {
                        restore_shadow(shadowed, id, was_shadowed);
                    }
                    crate::CaseBranch {
                        pattern: branch.pattern.clone(),
                        value,
                        span: branch.span,
                        coverage: branch.coverage,
                    }
                })
                .collect(),
        },
    };
    Expr {
        kind,
        ty: expression.ty,
        span: expression.span,
    }
}

fn restore_shadow(shadowed: &mut HashSet<LocalId>, id: LocalId, was_shadowed: bool) {
    if !was_shadowed {
        shadowed.remove(&id);
    }
}

fn pattern_ids(pattern: &Pattern) -> Vec<LocalId> {
    fn collect(pattern: &Pattern, ids: &mut Vec<LocalId>) {
        match &pattern.kind {
            PatternKind::Var { id, .. } => ids.push(*id),
            PatternKind::Named { id, pattern } => {
                ids.push(*id);
                collect(pattern, ids);
            }
            PatternKind::Array { elements } => {
                for element in elements {
                    collect(element, ids);
                }
            }
            PatternKind::Constructor { arguments, .. } => {
                for argument in arguments {
                    collect(argument, ids);
                }
            }
            PatternKind::Record { fields } => {
                for (_, field) in fields {
                    collect(field, ids);
                }
            }
            PatternKind::Wildcard | PatternKind::Literal { .. } => {}
        }
    }
    let mut ids = Vec::new();
    collect(pattern, &mut ids);
    ids
}

use crate::{Binding, Expr, ExprKind, LocalId, Module, Pattern, PatternKind};
use std::collections::{HashMap, HashSet};

pub(super) struct FreshLocals {
    next: Option<u32>,
}

impl FreshLocals {
    pub fn for_declaration(expression: &Expr) -> Self {
        let mut ids = HashSet::new();
        collect_ids(expression, &mut ids);
        Self {
            next: ids
                .iter()
                .map(|id| id.0)
                .max()
                .map_or(Some(0), |value| value.checked_add(1)),
        }
    }

    pub fn fresh(&mut self) -> Option<LocalId> {
        let id = LocalId(self.next?);
        self.next = id.0.checked_add(1);
        Some(id)
    }
}

pub(super) fn count_nodes(expression: &Expr) -> usize {
    1 + match &expression.kind {
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => 0,
        ExprKind::Constructor { arguments, .. }
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
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => count_nodes(record),
        ExprKind::UnaryPrimitive { value, .. } => count_nodes(value),
        ExprKind::ArrayIndex { array, index } => count_nodes(array) + count_nodes(index),
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => count_nodes(array) + count_nodes(index) + count_nodes(value),
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            count_nodes(left) + count_nodes(right)
        }
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
        ExprKind::Integer(value) => ExprKind::Integer(*value),
        ExprKind::Number(value) => ExprKind::Number(value.clone()),
        ExprKind::Boolean(value) => ExprKind::Boolean(*value),
        ExprKind::String(value) => ExprKind::String(value.clone()),
        ExprKind::Char(value) => ExprKind::Char(*value),
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
        ExprKind::ArrayLength(array) => {
            ExprKind::ArrayLength(Box::new(substitute_inner(array, substitutions, shadowed)))
        }
        ExprKind::UnaryPrimitive { op, value } => ExprKind::UnaryPrimitive {
            op: *op,
            value: Box::new(substitute_inner(value, substitutions, shadowed)),
        },
        ExprKind::ArrayIndex { array, index } => ExprKind::ArrayIndex {
            array: Box::new(substitute_inner(array, substitutions, shadowed)),
            index: Box::new(substitute_inner(index, substitutions, shadowed)),
        },
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => ExprKind::ArrayUpdate {
            array: Box::new(substitute_inner(array, substitutions, shadowed)),
            index: Box::new(substitute_inner(index, substitutions, shadowed)),
            value: Box::new(substitute_inner(value, substitutions, shadowed)),
        },
        ExprKind::Primitive { op, left, right } => ExprKind::Primitive {
            op: *op,
            left: Box::new(substitute_inner(left, substitutions, shadowed)),
            right: Box::new(substitute_inner(right, substitutions, shadowed)),
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
            PatternKind::Wildcard => {}
        }
    }
    let mut ids = Vec::new();
    collect(pattern, &mut ids);
    ids
}

fn collect_ids(expression: &Expr, ids: &mut HashSet<LocalId>) {
    match &expression.kind {
        ExprKind::Local(id) => {
            ids.insert(*id);
        }
        ExprKind::Lambda { binder, body } => {
            ids.insert(binder.id);
            collect_ids(body, ids);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                ids.insert(binding.binder.id);
                collect_ids(&binding.value, ids);
            }
            collect_ids(body, ids);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_ids(scrutinee, ids);
            for branch in branches {
                collect_pattern_ids(&branch.pattern, ids);
                collect_ids(&branch.value, ids);
            }
        }
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => {
            for argument in arguments {
                collect_ids(argument, ids);
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                collect_ids(value, ids);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            collect_ids(record, ids);
            for (_, value) in fields {
                collect_ids(value, ids);
            }
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            collect_ids(record, ids)
        }
        ExprKind::UnaryPrimitive { value, .. } => collect_ids(value, ids),
        ExprKind::ArrayIndex { array, index } => {
            collect_ids(array, ids);
            collect_ids(index, ids);
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            collect_ids(array, ids);
            collect_ids(index, ids);
            collect_ids(value, ids);
        }
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            collect_ids(left, ids);
            collect_ids(right, ids);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_ids(condition, ids);
            collect_ids(then_branch, ids);
            collect_ids(else_branch, ids);
        }
        ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}

fn collect_pattern_ids(pattern: &Pattern, ids: &mut HashSet<LocalId>) {
    match &pattern.kind {
        PatternKind::Var { id, .. } => {
            ids.insert(*id);
        }
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                collect_pattern_ids(argument, ids);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                collect_pattern_ids(field, ids);
            }
        }
        PatternKind::Wildcard => {}
    }
}

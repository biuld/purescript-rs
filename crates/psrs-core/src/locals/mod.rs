//! Shared collision-free local allocation for Core rewrites.
mod alpha;
pub(crate) use alpha::clone_with_fresh_locals;

use crate::{Expr, ExprKind, LocalId, Pattern, PatternKind};
use std::collections::HashSet;

pub(crate) struct FreshLocals {
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
        | ExprKind::IntrinsicCall { arguments, .. }
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
        ExprKind::FieldAccess { record, .. }
        | ExprKind::RepresentationCast { value: record, .. } => collect_ids(record, ids),
        ExprKind::Application(left, right) => {
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
        ExprKind::Unit | ExprKind::StateToken | ExprKind::Trap => {}
    }
}

fn collect_pattern_ids(pattern: &Pattern, ids: &mut HashSet<LocalId>) {
    match &pattern.kind {
        PatternKind::Var { id, .. } => {
            ids.insert(*id);
        }
        PatternKind::Named { id, pattern } => {
            ids.insert(*id);
            collect_pattern_ids(pattern, ids);
        }
        PatternKind::Array { elements } => {
            for element in elements {
                collect_pattern_ids(element, ids);
            }
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
        PatternKind::Wildcard | PatternKind::Literal { .. } => {}
    }
}

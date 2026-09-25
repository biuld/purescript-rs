use psrs_core::{Expr, ExprKind, PatternKind};
use psrs_hir::LocalId;
use std::collections::HashSet;

pub(super) fn lambda_captures(body: &Expr, binder: LocalId) -> Vec<LocalId> {
    let mut bound = HashSet::from([binder]);
    let mut captures = Vec::new();
    collect_captures(body, &mut bound, &mut captures);
    captures
}

pub(in crate::cc::lower) fn collect_captures(
    expression: &Expr,
    bound: &mut HashSet<LocalId>,
    captures: &mut Vec<LocalId>,
) {
    match &expression.kind {
        ExprKind::Local(local) => {
            if !bound.contains(local) && !captures.contains(local) {
                captures.push(*local);
            }
        }
        ExprKind::Lambda { binder, body } => {
            let inserted = bound.insert(binder.id);
            collect_captures(body, bound, captures);
            if inserted {
                bound.remove(&binder.id);
            }
        }
        ExprKind::Let { bindings, body } => {
            let mut nested = bound.clone();
            for binding in bindings {
                nested.insert(binding.binder.id);
            }
            for binding in bindings {
                collect_captures(&binding.value, &mut nested, captures);
            }
            collect_captures(body, &mut nested, captures);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_captures(scrutinee, bound, captures);
            for branch in branches {
                let mut nested = bound.clone();
                collect_pattern_locals(&branch.pattern.kind, &mut nested);
                collect_captures(&branch.value, &mut nested, captures);
            }
        }
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => {
            for argument in arguments {
                collect_captures(argument, bound, captures);
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                collect_captures(value, bound, captures);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            collect_captures(record, bound, captures);
            for (_, value) in fields {
                collect_captures(value, bound, captures);
            }
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            collect_captures(record, bound, captures)
        }
        ExprKind::ArrayIndex { array, index } => {
            collect_captures(array, bound, captures);
            collect_captures(index, bound, captures);
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            collect_captures(array, bound, captures);
            collect_captures(index, bound, captures);
            collect_captures(value, bound, captures);
        }
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            collect_captures(left, bound, captures);
            collect_captures(right, bound, captures);
        }
        ExprKind::UnaryPrimitive { value, .. } => collect_captures(value, bound, captures),
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_captures(condition, bound, captures);
            collect_captures(then_branch, bound, captures);
            collect_captures(else_branch, bound, captures);
        }
        ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}

fn collect_pattern_locals(pattern: &PatternKind, bound: &mut HashSet<LocalId>) {
    match pattern {
        PatternKind::Var { id, .. } => {
            bound.insert(*id);
        }
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                collect_pattern_locals(&argument.kind, bound);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                collect_pattern_locals(&field.kind, bound);
            }
        }
        PatternKind::Wildcard => {}
    }
}

use super::super::*;

/// The first local ID not used by any source-local binder in the module, used
/// to synthesize dictionary parameters.
pub(in crate::typecheck) fn next_local_id(module: &hir::Module) -> u32 {
    let mut max = None;
    for declaration in &module.declarations {
        scan_expr(&declaration.value, &mut max);
    }
    for instance in &module.instances {
        for member in &instance.members {
            scan_expr(&member.value, &mut max);
        }
    }
    max.map_or(0, |value| value + 1)
}

fn note_local(id: LocalId, max: &mut Option<u32>) {
    *max = Some(max.map_or(id.0, |value| value.max(id.0)));
}

fn scan_expr(expression: &hir::Expr, max: &mut Option<u32>) {
    match &expression.kind {
        hir::ExprKind::Local(id) => note_local(*id, max),
        hir::ExprKind::Global(_)
        | hir::ExprKind::Integer(_)
        | hir::ExprKind::Number(_)
        | hir::ExprKind::String(_)
        | hir::ExprKind::Char(_) => {}
        hir::ExprKind::Array(elements) => {
            for element in elements {
                scan_expr(element, max);
            }
        }
        hir::ExprKind::Record(fields) => {
            for (_, value) in fields {
                scan_expr(value, max);
            }
        }
        hir::ExprKind::RecordUpdate { expression, fields } => {
            scan_expr(expression, max);
            for (_, value) in fields {
                scan_expr(value, max);
            }
        }
        hir::ExprKind::FieldAccess { expression, .. } => scan_expr(expression, max),
        hir::ExprKind::Application(function, argument) => {
            scan_expr(function, max);
            scan_expr(argument, max);
        }
        hir::ExprKind::Operator { left, right, .. } => {
            scan_expr(left, max);
            scan_expr(right, max);
        }
        hir::ExprKind::Lambda { binder, body } => {
            note_local(binder.id, max);
            scan_expr(body, max);
        }
        hir::ExprKind::Let { bindings, body } => {
            for binding in bindings {
                note_local(binding.binder.id, max);
                scan_expr(&binding.value, max);
            }
            scan_expr(body, max);
        }
        hir::ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            scan_expr(condition, max);
            scan_expr(then_branch, max);
            scan_expr(else_branch, max);
        }
        hir::ExprKind::Case {
            scrutinee,
            branches,
        } => {
            scan_expr(scrutinee, max);
            for branch in branches {
                scan_pattern(&branch.pattern, max);
                scan_expr(&branch.value, max);
            }
        }
    }
}

fn scan_pattern(pattern: &hir::Pattern, max: &mut Option<u32>) {
    match &pattern.kind {
        hir::PatternKind::Wildcard => {}
        hir::PatternKind::Var(binder) => note_local(binder.id, max),
        hir::PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                scan_pattern(argument, max);
            }
        }
        hir::PatternKind::Record { fields } => {
            for (_, field) in fields {
                scan_pattern(field, max);
            }
        }
    }
}

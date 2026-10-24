//! Exhaustive traversal of Core type references, without changing binders.

use crate::{Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeId};

pub(super) fn children(ty: &mut Type, visit: &mut impl FnMut(&mut TypeId)) {
    match ty {
        Type::Application(function, argument) => {
            visit(function);
            visit(argument);
        }
        Type::ForAll { body, .. } => visit(body),
        Type::RowExtend { ty, tail, .. } => {
            visit(ty);
            visit(tail);
        }
        Type::Closure { parameters, result } => {
            for parameter in parameters {
                visit(parameter);
            }
            visit(result);
        }
        Type::Variable(_)
        | Type::Constructor(_)
        | Type::RowEmpty
        | Type::TypeLevelString(_)
        | Type::TypeLevelInt(_) => {}
    }
}

pub(super) fn roots(module: &mut Module, visit: &mut impl FnMut(&mut TypeId)) {
    for external in &mut module.external_types {
        visit(&mut external.ty);
    }
    for constructor in &mut module.constructors {
        for field in &mut constructor.field_types {
            visit(field);
        }
    }
    for declaration in &mut module.declarations {
        visit(&mut declaration.ty);
        expression(&mut declaration.value, visit);
    }
}

fn expression(expr: &mut Expr, visit: &mut impl FnMut(&mut TypeId)) {
    psrs_span::with_sufficient_stack(|| {
        visit(&mut expr.ty);
        match &mut expr.kind {
            ExprKind::Local(_)
            | ExprKind::Global(_)
            | ExprKind::Integer(_)
            | ExprKind::Number(_)
            | ExprKind::Boolean(_)
            | ExprKind::String(_)
            | ExprKind::Char(_)
            | ExprKind::Unit
            | ExprKind::Trap => {}
            ExprKind::Constructor { arguments, .. }
            | ExprKind::IntrinsicCall { arguments, .. }
            | ExprKind::Array {
                elements: arguments,
            } => {
                for argument in arguments {
                    expression(argument, visit);
                }
            }
            ExprKind::Record { fields } => {
                for (_, value) in fields {
                    expression(value, visit);
                }
            }
            ExprKind::RecordUpdate { record, fields } => {
                expression(record, visit);
                for (_, value) in fields {
                    expression(value, visit);
                }
            }
            ExprKind::FieldAccess { record, .. } => expression(record, visit),
            ExprKind::RepresentationCast {
                value,
                source_type,
                target_type,
            } => {
                expression(value, visit);
                visit(source_type);
                visit(target_type);
            }
            ExprKind::Application(function, argument) => {
                expression(function, visit);
                expression(argument, visit);
            }
            ExprKind::Lambda { binder, body } => {
                visit(&mut binder.ty);
                expression(body, visit);
            }
            ExprKind::Let { bindings, body } => {
                for binding in bindings {
                    visit(&mut binding.binder.ty);
                    expression(&mut binding.value, visit);
                }
                expression(body, visit);
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                expression(condition, visit);
                expression(then_branch, visit);
                expression(else_branch, visit);
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                expression(scrutinee, visit);
                for branch in branches {
                    pattern(&mut branch.pattern, visit);
                    expression(&mut branch.value, visit);
                }
            }
        }
    });
}

fn pattern(node: &mut Pattern, visit: &mut impl FnMut(&mut TypeId)) {
    psrs_span::with_sufficient_stack(|| {
        visit(&mut node.ty);
        match &mut node.kind {
            PatternKind::Wildcard | PatternKind::Literal { .. } => {}
            PatternKind::Var { ty, .. } => visit(ty),
            PatternKind::Named { pattern: inner, .. } => pattern(inner, visit),
            PatternKind::Array { elements }
            | PatternKind::Constructor {
                arguments: elements,
                ..
            } => {
                for element in elements {
                    pattern(element, visit);
                }
            }
            PatternKind::Record { fields } => {
                for (_, value) in fields {
                    pattern(value, visit);
                }
            }
        }
    });
}

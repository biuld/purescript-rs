use psrs_core::{Expr, ExprKind, Module as CoreModule, Pattern, PatternKind, Type, TypeId};
use std::collections::HashSet;

/// Callable types that occur on a remaining declaration, expression, or
/// constructor field, or reserved aggregate layout. Other residual callable
/// types left behind by pruned library code are omitted.
pub(super) fn referenced_types(
    module: &CoreModule,
    aggregate_roots: impl Iterator<Item = TypeId>,
) -> HashSet<TypeId> {
    let mut referenced = HashSet::new();
    let mut visiting = HashSet::new();
    // Every reserved aggregate is normalized by the layout builder. Its nested
    // callable fields therefore need signatures even when its Core type is a
    // residual template without a remaining expression of that exact type.
    for ty in aggregate_roots {
        record_type(module, ty, &mut visiting, &mut referenced);
    }
    for declaration in &module.declarations {
        record_type(module, declaration.ty, &mut visiting, &mut referenced);
        record_expr(module, &declaration.value, &mut visiting, &mut referenced);
    }
    for constructor in &module.constructors {
        for field in &constructor.field_types {
            record_type(module, *field, &mut visiting, &mut referenced);
        }
    }
    referenced
}

fn record_expr(
    module: &CoreModule,
    expression: &Expr,
    visiting: &mut HashSet<TypeId>,
    referenced: &mut HashSet<TypeId>,
) {
    record_type(module, expression.ty, visiting, referenced);
    match &expression.kind {
        ExprKind::Array { elements } => {
            for element in elements {
                record_expr(module, element, visiting, referenced);
            }
        }
        ExprKind::Record { fields } | ExprKind::RecordUpdate { fields, .. } => {
            if let ExprKind::RecordUpdate { record, .. } = &expression.kind {
                record_expr(module, record, visiting, referenced);
            }
            for (_, value) in fields {
                record_expr(module, value, visiting, referenced);
            }
        }
        ExprKind::FieldAccess { record, .. } => {
            record_expr(module, record, visiting, referenced);
        }
        ExprKind::Constructor { arguments, .. } | ExprKind::IntrinsicCall { arguments, .. } => {
            for argument in arguments {
                record_expr(module, argument, visiting, referenced);
            }
        }
        ExprKind::Application(function, argument) => {
            record_expr(module, function, visiting, referenced);
            record_expr(module, argument, visiting, referenced);
        }
        ExprKind::RepresentationCast { value, .. } => {
            record_expr(module, value, visiting, referenced);
        }
        ExprKind::Lambda { binder, body } => {
            record_type(module, binder.ty, visiting, referenced);
            record_expr(module, body, visiting, referenced);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                record_type(module, binding.binder.ty, visiting, referenced);
                record_expr(module, &binding.value, visiting, referenced);
            }
            record_expr(module, body, visiting, referenced);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            record_expr(module, condition, visiting, referenced);
            record_expr(module, then_branch, visiting, referenced);
            record_expr(module, else_branch, visiting, referenced);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            record_expr(module, scrutinee, visiting, referenced);
            for branch in branches {
                record_pattern(module, &branch.pattern, visiting, referenced);
                record_expr(module, &branch.value, visiting, referenced);
            }
        }
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
        // Both mention no subexpression, so the type recorded above is all
        // their layout can refer to.
        ExprKind::Unit | ExprKind::Trap => {}
    }
}

fn record_pattern(
    module: &CoreModule,
    pattern: &Pattern,
    visiting: &mut HashSet<TypeId>,
    referenced: &mut HashSet<TypeId>,
) {
    record_type(module, pattern.ty, visiting, referenced);
    match &pattern.kind {
        PatternKind::Var { ty, .. } => record_type(module, *ty, visiting, referenced),
        PatternKind::Named { pattern, .. } => record_pattern(module, pattern, visiting, referenced),
        PatternKind::Array { elements } => {
            for element in elements {
                record_pattern(module, element, visiting, referenced);
            }
        }
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                record_pattern(module, argument, visiting, referenced);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                record_pattern(module, field, visiting, referenced);
            }
        }
        PatternKind::Wildcard | PatternKind::Literal { .. } => {}
    }
}

fn record_type(
    module: &CoreModule,
    id: TypeId,
    visiting: &mut HashSet<TypeId>,
    referenced: &mut HashSet<TypeId>,
) {
    if !visiting.insert(id) {
        return;
    }
    match module.types.get(id.0 as usize) {
        Some(Type::Application(function, argument)) => {
            // An ordinary arrow and a registered callable constructor
            // application are both callable closures and need a representation
            // signature even though they are not a dedicated type node.
            if super::super::is_callable_type(module, id) {
                referenced.insert(id);
            }
            record_type(module, *function, visiting, referenced);
            record_type(module, *argument, visiting, referenced);
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            record_type(module, *ty, visiting, referenced);
            record_type(module, *tail, visiting, referenced);
        }
        Some(Type::ForAll { body, .. }) => {
            // Keep a signature entry for the quantified value itself as well
            // as for its body. Use sites retain the scheme TypeId on binders,
            // while expression TypeIds may name its instantiated body.
            if super::super::is_callable_type(module, id) {
                referenced.insert(id);
            }
            record_type(module, *body, visiting, referenced);
        }
        Some(Type::Closure { parameters, result }) => {
            referenced.insert(id);
            for parameter in parameters {
                record_type(module, *parameter, visiting, referenced);
            }
            record_type(module, *result, visiting, referenced);
        }
        _ => {}
    }
}

use psrs_core::{Expr, ExprKind, Module as CoreModule, Type, TypeId};
use std::collections::HashSet;

/// Callable types that occur on a remaining declaration, expression, or
/// constructor field. Types left behind by pruned library code are omitted.
pub(super) fn referenced_types(module: &CoreModule) -> HashSet<TypeId> {
    let mut referenced = HashSet::new();
    let mut visiting = HashSet::new();
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
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            record_expr(module, record, visiting, referenced);
        }
        ExprKind::ArrayIndex { array, index } => {
            record_expr(module, array, visiting, referenced);
            record_expr(module, index, visiting, referenced);
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            record_expr(module, array, visiting, referenced);
            record_expr(module, index, visiting, referenced);
            record_expr(module, value, visiting, referenced);
        }
        ExprKind::Constructor { arguments, .. } => {
            for argument in arguments {
                record_expr(module, argument, visiting, referenced);
            }
        }
        ExprKind::Application(function, argument)
        | ExprKind::Primitive {
            left: function,
            right: argument,
            ..
        } => {
            record_expr(module, function, visiting, referenced);
            record_expr(module, argument, visiting, referenced);
        }
        ExprKind::UnaryPrimitive { value, .. } => {
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
                record_type(module, branch.pattern.ty, visiting, referenced);
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
        _ => {}
    }
}

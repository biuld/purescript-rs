use psrs_core::{Expr, ExprKind, Module as CoreModule, Pattern, PatternKind, Type};
use psrs_hir::LocalId;
use std::collections::HashSet;

pub(super) fn module_has_integer_capture(module: &CoreModule) -> bool {
    module
        .declarations
        .iter()
        .any(|declaration| expression_has_integer_capture(&declaration.value, module))
}

fn expression_has_integer_capture(expression: &Expr, module: &CoreModule) -> bool {
    match &expression.kind {
        ExprKind::Lambda { binder, body } => {
            let mut bound = HashSet::from([binder.id]);
            free_integer_local(body, module, &mut bound)
                || expression_has_integer_capture(body, module)
        }
        ExprKind::Array { elements } => elements
            .iter()
            .any(|element| expression_has_integer_capture(element, module)),
        ExprKind::Record { fields } => fields
            .iter()
            .any(|(_, value)| expression_has_integer_capture(value, module)),
        ExprKind::RecordUpdate { record, fields } => {
            expression_has_integer_capture(record, module)
                || fields
                    .iter()
                    .any(|(_, value)| expression_has_integer_capture(value, module))
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            expression_has_integer_capture(record, module)
        }
        ExprKind::ArrayIndex { array, index } => {
            expression_has_integer_capture(array, module)
                || expression_has_integer_capture(index, module)
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            expression_has_integer_capture(array, module)
                || expression_has_integer_capture(index, module)
                || expression_has_integer_capture(value, module)
        }
        ExprKind::Constructor { arguments, .. } => arguments
            .iter()
            .any(|argument| expression_has_integer_capture(argument, module)),
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            expression_has_integer_capture(left, module)
                || expression_has_integer_capture(right, module)
        }
        ExprKind::UnaryPrimitive { value, .. } => expression_has_integer_capture(value, module),
        ExprKind::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| expression_has_integer_capture(&binding.value, module))
                || expression_has_integer_capture(body, module)
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expression_has_integer_capture(condition, module)
                || expression_has_integer_capture(then_branch, module)
                || expression_has_integer_capture(else_branch, module)
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            expression_has_integer_capture(scrutinee, module)
                || branches
                    .iter()
                    .any(|branch| expression_has_integer_capture(&branch.value, module))
        }
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => false,
    }
}

fn free_integer_local(
    expression: &Expr,
    module: &CoreModule,
    bound: &mut HashSet<LocalId>,
) -> bool {
    match &expression.kind {
        ExprKind::Local(id) => {
            !bound.contains(id)
                && matches!(module.types.get(expression.ty.0 as usize), Some(Type::I32))
        }
        ExprKind::Lambda { binder, body } => {
            let inserted = bound.insert(binder.id);
            let found = free_integer_local(body, module, bound);
            if inserted {
                bound.remove(&binder.id);
            }
            found
        }
        ExprKind::Let { bindings, body } => {
            let inserted = bindings
                .iter()
                .map(|binding| (binding.binder.id, bound.insert(binding.binder.id)))
                .collect::<Vec<_>>();
            let found = bindings
                .iter()
                .any(|binding| free_integer_local(&binding.value, module, bound))
                || free_integer_local(body, module, bound);
            for (id, was_inserted) in inserted {
                if was_inserted {
                    bound.remove(&id);
                }
            }
            found
        }
        ExprKind::Array { elements } => elements
            .iter()
            .any(|element| free_integer_local(element, module, bound)),
        ExprKind::Record { fields } => fields
            .iter()
            .any(|(_, value)| free_integer_local(value, module, bound)),
        ExprKind::RecordUpdate { record, fields } => {
            free_integer_local(record, module, bound)
                || fields
                    .iter()
                    .any(|(_, value)| free_integer_local(value, module, bound))
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            free_integer_local(record, module, bound)
        }
        ExprKind::ArrayIndex { array, index } => {
            free_integer_local(array, module, bound) || free_integer_local(index, module, bound)
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            free_integer_local(array, module, bound)
                || free_integer_local(index, module, bound)
                || free_integer_local(value, module, bound)
        }
        ExprKind::Constructor { arguments, .. } => arguments
            .iter()
            .any(|argument| free_integer_local(argument, module, bound)),
        ExprKind::Primitive { left, right, .. } | ExprKind::Application(left, right) => {
            free_integer_local(left, module, bound) || free_integer_local(right, module, bound)
        }
        ExprKind::UnaryPrimitive { value, .. } => free_integer_local(value, module, bound),
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            free_integer_local(condition, module, bound)
                || free_integer_local(then_branch, module, bound)
                || free_integer_local(else_branch, module, bound)
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            if free_integer_local(scrutinee, module, bound) {
                return true;
            }
            branches.iter().any(|branch| {
                let mut branch_bound = bound.clone();
                bind_pattern(&branch.pattern, &mut branch_bound);
                free_integer_local(&branch.value, module, &mut branch_bound)
            })
        }
        ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => false,
    }
}

fn bind_pattern(pattern: &Pattern, bound: &mut HashSet<LocalId>) {
    match &pattern.kind {
        PatternKind::Wildcard => {}
        PatternKind::Var { id, .. } => {
            bound.insert(*id);
        }
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                bind_pattern(argument, bound);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                bind_pattern(field, bound);
            }
        }
    }
}

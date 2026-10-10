use super::{enter, error, leading_foralls, types::scoped_type};
use crate::{Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeId, VerifyError};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

pub(super) fn scoped_expr(
    expression: &Expr,
    module: &Module,
    scope: &mut HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    psrs_span::with_sufficient_stack(|| scoped_expr_inner(expression, module, scope, errors))
}

fn scoped_expr_inner(
    expression: &Expr,
    module: &Module,
    scope: &mut HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    scoped_type(
        expression.ty,
        module,
        scope,
        expression.span,
        &mut HashSet::new(),
        errors,
    );
    match &expression.kind {
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
        // Both are leaves: their type is already checked by the expression
        // check, and neither mentions a binder.
        ExprKind::Unit | ExprKind::StateToken | ExprKind::Trap => {}
        ExprKind::Constructor { arguments, .. } => {
            // Constructor lowering collapses a THIR application spine. Its
            // result quantifiers still bind the corresponding free variables
            // in instantiated constructor fields, just as for Application.
            let binders = leading_foralls(module, expression.ty);
            for argument in arguments {
                let mut argument_scope = scope.clone();
                open_child_binders(argument, &binders, module, &mut argument_scope, errors);
                scoped_expr(argument, module, &mut argument_scope, errors);
            }
        }
        ExprKind::IntrinsicCall { arguments, .. } => {
            for argument in arguments {
                scoped_expr(argument, module, scope, errors);
            }
        }
        ExprKind::Array { elements } => {
            let binders = leading_foralls(module, expression.ty);
            for element in elements {
                let mut element_scope = scope.clone();
                open_child_binders(element, &binders, module, &mut element_scope, errors);
                scoped_expr(element, module, &mut element_scope, errors);
            }
        }
        ExprKind::Record { fields } => {
            let field_types = module.record_fields(expression.ty).unwrap_or_default();
            for (label, value) in fields {
                let binders = field_types
                    .iter()
                    .find(|(field_label, _)| field_label == label)
                    .map(|(_, ty)| leading_foralls(module, *ty))
                    .unwrap_or_default();
                let mut field_scope = scope.clone();
                open_child_binders(value, &binders, module, &mut field_scope, errors);
                scoped_expr(value, module, &mut field_scope, errors);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            scoped_expr(record, module, scope, errors);
            for (_, value) in fields {
                scoped_expr(value, module, scope, errors);
            }
        }
        ExprKind::FieldAccess { record, .. } => {
            let binders = leading_foralls(module, expression.ty);
            let mut record_scope = scope.clone();
            open_child_binders(record, &binders, module, &mut record_scope, errors);
            scoped_expr(record, module, &mut record_scope, errors);
        }
        ExprKind::RepresentationCast {
            value,
            source_type,
            target_type,
        } => {
            scoped_expr(value, module, scope, errors);
            scoped_type(
                *source_type,
                module,
                scope,
                expression.span,
                &mut HashSet::new(),
                errors,
            );
            scoped_type(
                *target_type,
                module,
                scope,
                expression.span,
                &mut HashSet::new(),
                errors,
            );
        }
        ExprKind::Application(function, argument) => {
            let binders = leading_foralls(module, expression.ty);
            let mut function_scope = scope.clone();
            open_child_binders(function, &binders, module, &mut function_scope, errors);
            scoped_expr(function, module, &mut function_scope, errors);
            let mut argument_scope = scope.clone();
            open_child_binders(argument, &binders, module, &mut argument_scope, errors);
            scoped_expr(argument, module, &mut argument_scope, errors);
        }
        ExprKind::Lambda { binder, body } => {
            let mut body_scope = scope.clone();
            open_expression_binders(expression, module, &mut body_scope, errors);
            scoped_type(
                binder.ty,
                module,
                &body_scope,
                binder.span,
                &mut HashSet::new(),
                errors,
            );
            scoped_expr(body, module, &mut body_scope, errors);
        }
        ExprKind::Let { bindings, body } => {
            let mut body_scope = scope.clone();
            open_expression_binders(expression, module, &mut body_scope, errors);
            for binding in bindings {
                let mut binding_scope = body_scope.clone();
                enter(
                    &binding.quantified,
                    &mut binding_scope,
                    module.id,
                    binding.span,
                    "binding quantifiers must be unique and lexically distinct",
                    errors,
                );
                scoped_type(
                    binding.binder.ty,
                    module,
                    &binding_scope,
                    binding.binder.span,
                    &mut HashSet::new(),
                    errors,
                );
                scoped_expr(&binding.value, module, &mut binding_scope, errors);
            }
            scoped_expr(body, module, &mut body_scope, errors);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            scoped_expr(condition, module, scope, errors);
            let mut branch_scope = scope.clone();
            open_expression_binders(expression, module, &mut branch_scope, errors);
            scoped_expr(then_branch, module, &mut branch_scope, errors);
            scoped_expr(else_branch, module, &mut branch_scope, errors);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            scoped_expr(scrutinee, module, scope, errors);
            let mut branch_scope = scope.clone();
            open_expression_binders(expression, module, &mut branch_scope, errors);
            for branch in branches {
                scoped_pattern(&branch.pattern, module, &branch_scope, errors);
                scoped_expr(&branch.value, module, &mut branch_scope, errors);
            }
        }
    }
}

fn open_expression_binders(
    expression: &Expr,
    module: &Module,
    scope: &mut HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    let binders = leading_foralls(module, expression.ty);
    if binders.is_empty() {
        return;
    }
    let mut local = HashSet::new();
    if binders
        .iter()
        .any(|variable| !local.insert(*variable) || scope.contains(variable))
    {
        errors.push(error(
            module,
            expression.span,
            "forall binder shadows an active type variable",
        ));
    }
    scope.extend(local);
}

fn open_child_binders(
    child: &Expr,
    candidates: &[TypeVariableId],
    module: &Module,
    scope: &mut HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    let mut free = HashSet::new();
    free_type_variables(
        child.ty,
        module,
        &mut HashMap::new(),
        &mut HashSet::new(),
        &mut free,
    );
    let relevant = candidates
        .iter()
        .copied()
        .filter(|variable| free.contains(variable))
        .collect::<Vec<_>>();
    if relevant.is_empty() {
        return;
    }
    let mut local = HashSet::new();
    if relevant
        .iter()
        .any(|variable| !local.insert(*variable) || scope.contains(variable))
    {
        errors.push(error(
            module,
            child.span,
            "forall binder shadows an active type variable",
        ));
    }
    scope.extend(local);
}

fn free_type_variables(
    id: TypeId,
    module: &Module,
    bound: &mut HashMap<TypeVariableId, usize>,
    active: &mut HashSet<TypeId>,
    free: &mut HashSet<TypeVariableId>,
) {
    if !active.insert(id) {
        return;
    }
    match module.types.get(id.0 as usize) {
        Some(Type::Variable(variable)) if !bound.contains_key(variable) => {
            free.insert(*variable);
        }
        Some(Type::Application(function, argument)) => {
            free_type_variables(*function, module, bound, active, free);
            free_type_variables(*argument, module, bound, active, free);
        }
        Some(Type::ForAll { variables, body }) => {
            for variable in variables {
                *bound.entry(*variable).or_default() += 1;
            }
            free_type_variables(*body, module, bound, active, free);
            for variable in variables {
                if let Some(count) = bound.get_mut(variable) {
                    *count -= 1;
                    if *count == 0 {
                        bound.remove(variable);
                    }
                }
            }
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            free_type_variables(*ty, module, bound, active, free);
            free_type_variables(*tail, module, bound, active, free);
        }
        Some(Type::Closure { parameters, result }) => {
            for parameter in parameters {
                free_type_variables(*parameter, module, bound, active, free);
            }
            free_type_variables(*result, module, bound, active, free);
        }
        _ => {}
    }
    active.remove(&id);
}

fn scoped_pattern(
    pattern: &Pattern,
    module: &Module,
    scope: &HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    scoped_type(
        pattern.ty,
        module,
        scope,
        pattern.span,
        &mut HashSet::new(),
        errors,
    );
    match &pattern.kind {
        PatternKind::Wildcard | PatternKind::Literal { .. } => {}
        PatternKind::Var { ty, .. } => scoped_type(
            *ty,
            module,
            scope,
            pattern.span,
            &mut HashSet::new(),
            errors,
        ),
        PatternKind::Array { elements } => {
            for element in elements {
                scoped_pattern(element, module, scope, errors);
            }
        }
        PatternKind::Named { pattern, .. } => scoped_pattern(pattern, module, scope, errors),
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                scoped_pattern(argument, module, scope, errors);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                scoped_pattern(field, module, scope, errors);
            }
        }
    }
}

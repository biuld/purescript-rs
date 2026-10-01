use crate::{Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeId, VerifyError};
use psrs_hir::TypeVariableId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

pub(super) fn verify_module(module: &Module) -> Vec<VerifyError> {
    let mut errors = Vec::new();
    let mut complete = HashSet::new();
    for index in 0..module.types.len() {
        acyclic(
            TypeId(index as u32),
            module,
            &mut HashSet::new(),
            &mut complete,
            module.span,
            &mut errors,
        );
    }
    for constructor in &module.constructors {
        let mut scope = HashSet::new();
        enter(
            &constructor.parameters,
            &mut scope,
            module.id,
            module.span,
            "constructor type parameters must be unique",
            &mut errors,
        );
        for field in &constructor.field_types {
            scoped_type(
                *field,
                module,
                &scope,
                module.span,
                &mut HashSet::new(),
                &mut errors,
            );
        }
    }
    for declaration in &module.declarations {
        let mut scope = HashSet::new();
        enter(
            &declaration.quantified,
            &mut scope,
            module.id,
            declaration.name_span,
            "declaration quantifiers must be unique",
            &mut errors,
        );
        scoped_type(
            declaration.ty,
            module,
            &scope,
            declaration.name_span,
            &mut HashSet::new(),
            &mut errors,
        );
        scoped_expr(&declaration.value, module, &mut scope, &mut errors);
    }
    errors
}

fn acyclic(
    id: TypeId,
    module: &Module,
    active: &mut HashSet<TypeId>,
    complete: &mut HashSet<TypeId>,
    span: TextRange,
    errors: &mut Vec<VerifyError>,
) {
    if complete.contains(&id) {
        return;
    }
    if !active.insert(id) {
        errors.push(error(module, span, "type table contains a cycle"));
        return;
    }
    match module.types.get(id.0 as usize) {
        Some(Type::Application(function, argument)) => {
            acyclic(*function, module, active, complete, span, errors);
            acyclic(*argument, module, active, complete, span, errors);
        }
        Some(Type::ForAll { body, .. }) => acyclic(*body, module, active, complete, span, errors),
        Some(Type::RowExtend { ty, tail, .. }) => {
            acyclic(*ty, module, active, complete, span, errors);
            acyclic(*tail, module, active, complete, span, errors);
        }
        _ => {}
    }
    active.remove(&id);
    complete.insert(id);
}

fn scoped_type(
    id: TypeId,
    module: &Module,
    scope: &HashSet<TypeVariableId>,
    span: TextRange,
    active: &mut HashSet<TypeId>,
    errors: &mut Vec<VerifyError>,
) {
    if !active.insert(id) {
        return;
    }
    match module.types.get(id.0 as usize) {
        Some(Type::Variable(variable)) if !scope.contains(variable) => {
            errors.push(error(
                module,
                span,
                "type variable is outside its quantifier scope",
            ));
        }
        Some(Type::Application(function, argument)) => {
            scoped_type(*function, module, scope, span, active, errors);
            scoped_type(*argument, module, scope, span, active, errors);
        }
        Some(Type::ForAll { variables, body }) => {
            let mut nested = scope.clone();
            if variables.is_empty() {
                errors.push(error(
                    module,
                    span,
                    "forall binders must be non-empty, unique, and lexically distinct",
                ));
            }
            enter(
                variables,
                &mut nested,
                module.id,
                span,
                "forall binders must be non-empty, unique, and lexically distinct",
                errors,
            );
            scoped_type(*body, module, &nested, span, active, errors);
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            scoped_type(*ty, module, scope, span, active, errors);
            scoped_type(*tail, module, scope, span, active, errors);
        }
        _ => {}
    }
    active.remove(&id);
}

fn enter(
    variables: &[TypeVariableId],
    scope: &mut HashSet<TypeVariableId>,
    module: psrs_hir::ModuleId,
    span: TextRange,
    message: &'static str,
    errors: &mut Vec<VerifyError>,
) {
    let mut local = HashSet::new();
    if variables
        .iter()
        .any(|variable| !local.insert(*variable) || scope.contains(variable))
    {
        errors.push(VerifyError {
            module,
            span,
            message,
        });
    }
    scope.extend(local);
}

fn scoped_expr(
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
        ExprKind::Constructor { arguments, .. } => {
            for argument in arguments {
                scoped_expr(argument, module, scope, errors);
            }
        }
        ExprKind::Array { elements } => {
            for element in elements {
                scoped_expr(element, module, scope, errors);
            }
        }
        ExprKind::Record { fields } => {
            for (_, value) in fields {
                scoped_expr(value, module, scope, errors);
            }
        }
        ExprKind::RecordUpdate { record, fields } => {
            scoped_expr(record, module, scope, errors);
            for (_, value) in fields {
                scoped_expr(value, module, scope, errors);
            }
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            scoped_expr(record, module, scope, errors)
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
        ExprKind::ArrayIndex { array, index } => {
            scoped_expr(array, module, scope, errors);
            scoped_expr(index, module, scope, errors);
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            scoped_expr(array, module, scope, errors);
            scoped_expr(index, module, scope, errors);
            scoped_expr(value, module, scope, errors);
        }
        ExprKind::Primitive { left, right, .. } => {
            scoped_expr(left, module, scope, errors);
            scoped_expr(right, module, scope, errors);
        }
        ExprKind::UnaryPrimitive { value, .. } => scoped_expr(value, module, scope, errors),
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
            for binding in bindings {
                let mut binding_scope = scope.clone();
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
            let mut body_scope = scope.clone();
            open_expression_binders(expression, module, &mut body_scope, errors);
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
        PatternKind::Wildcard => {}
        PatternKind::Var { ty, .. } => scoped_type(
            *ty,
            module,
            scope,
            pattern.span,
            &mut HashSet::new(),
            errors,
        ),
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

fn leading_foralls(module: &Module, mut id: TypeId) -> Vec<TypeVariableId> {
    let mut binders = Vec::new();
    let mut seen = HashSet::new();
    while let Some(Type::ForAll { variables, body }) = module.types.get(id.0 as usize) {
        if !seen.insert(id) {
            break;
        }
        binders.extend(variables.iter().copied());
        id = *body;
    }
    binders
}

fn error(module: &Module, span: TextRange, message: &'static str) -> VerifyError {
    VerifyError {
        module: module.id,
        span,
        message,
    }
}

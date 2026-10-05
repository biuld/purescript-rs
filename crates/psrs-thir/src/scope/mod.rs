use crate::{
    Evidence, EvidenceKind, Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeId, VerifyError,
};
use psrs_hir::TypeVariableId;
use psrs_span::TextRange;
use std::collections::HashSet;

mod free_type_variables;
mod polymorphic;
use polymorphic::{leading_foralls, open_child_binders};

pub(super) fn verify_module(module: &Module) -> Vec<VerifyError> {
    let mut errors = Vec::new();
    let mut complete = HashSet::new();
    for index in 0..module.types.len() {
        verify_acyclic(
            TypeId(index as u32),
            &module.types,
            &mut HashSet::new(),
            &mut complete,
            module.span,
            &mut errors,
        );
    }
    for constructor in &module.constructors {
        let mut scope = HashSet::new();
        enter_binders(
            &constructor.parameters,
            &mut scope,
            module.span,
            "constructor type parameters must be unique",
            &mut errors,
        );
        for field in &constructor.field_types {
            verify_type_scope(
                *field,
                &module.types,
                &scope,
                module.span,
                &mut HashSet::new(),
                &mut errors,
            );
        }
    }
    for external in &module.external_types {
        verify_type_scope(
            external.ty,
            &module.types,
            &HashSet::new(),
            module.span,
            &mut HashSet::new(),
            &mut errors,
        );
    }
    for declaration in &module.declarations {
        let mut scope = HashSet::new();
        enter_binders(
            &declaration.quantified,
            &mut scope,
            declaration.name_span,
            "declaration quantifiers must be unique",
            &mut errors,
        );
        verify_type_scope(
            declaration.ty,
            &module.types,
            &scope,
            declaration.name_span,
            &mut HashSet::new(),
            &mut errors,
        );
        verify_expr_scope(&declaration.value, &module.types, &mut scope, &mut errors);
    }
    errors
}

fn verify_acyclic(
    id: TypeId,
    types: &[Type],
    active: &mut HashSet<TypeId>,
    complete: &mut HashSet<TypeId>,
    span: TextRange,
    errors: &mut Vec<VerifyError>,
) {
    if complete.contains(&id) {
        return;
    }
    if !active.insert(id) {
        errors.push(VerifyError {
            span,
            message: "type table contains a cycle",
        });
        return;
    }
    match types.get(id.0 as usize) {
        Some(Type::Application(function, argument)) => {
            verify_acyclic(*function, types, active, complete, span, errors);
            verify_acyclic(*argument, types, active, complete, span, errors);
        }
        Some(Type::ForAll { body, .. }) => {
            verify_acyclic(*body, types, active, complete, span, errors);
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            verify_acyclic(*ty, types, active, complete, span, errors);
            verify_acyclic(*tail, types, active, complete, span, errors);
        }
        _ => {}
    }
    active.remove(&id);
    complete.insert(id);
}

fn verify_type_scope(
    id: TypeId,
    types: &[Type],
    scope: &HashSet<TypeVariableId>,
    span: TextRange,
    active: &mut HashSet<TypeId>,
    errors: &mut Vec<VerifyError>,
) {
    if !active.insert(id) {
        return;
    }
    match types.get(id.0 as usize) {
        Some(Type::Variable(variable)) if !scope.contains(variable) => {
            errors.push(VerifyError {
                span,
                message: "type variable is outside its quantifier scope",
            });
        }
        Some(Type::Application(function, argument)) => {
            verify_type_scope(*function, types, scope, span, active, errors);
            verify_type_scope(*argument, types, scope, span, active, errors);
        }
        Some(Type::ForAll { variables, body }) => {
            let mut nested = scope.clone();
            if variables.is_empty() {
                errors.push(VerifyError {
                    span,
                    message: "forall binders must be non-empty, unique, and lexically distinct",
                });
            }
            enter_binders(
                variables,
                &mut nested,
                span,
                "forall binders must be non-empty, unique, and lexically distinct",
                errors,
            );
            verify_type_scope(*body, types, &nested, span, active, errors);
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            verify_type_scope(*ty, types, scope, span, active, errors);
            verify_type_scope(*tail, types, scope, span, active, errors);
        }
        _ => {}
    }
    active.remove(&id);
}

fn enter_binders(
    variables: &[TypeVariableId],
    scope: &mut HashSet<TypeVariableId>,
    span: TextRange,
    message: &'static str,
    errors: &mut Vec<VerifyError>,
) {
    let mut local = HashSet::new();
    let invalid = variables
        .iter()
        .any(|variable| !local.insert(*variable) || scope.contains(variable));
    if invalid {
        errors.push(VerifyError { span, message });
    }
    scope.extend(local);
}

fn verify_expr_scope(
    expression: &Expr,
    types: &[Type],
    scope: &mut HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    verify_type_scope(
        expression.ty,
        types,
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
        ExprKind::Array(elements) => {
            let binders = leading_foralls(types, expression.ty);
            for element in elements {
                let mut element_scope = scope.clone();
                open_child_binders(element, &binders, types, &mut element_scope, errors);
                verify_expr_scope(element, types, &mut element_scope, errors);
            }
        }
        ExprKind::Record(fields) => {
            let field_types = crate::record_fields(types, expression.ty).unwrap_or_default();
            for (label, value) in fields {
                let binders = field_types
                    .iter()
                    .find(|(field_label, _)| field_label == label)
                    .map(|(_, ty)| leading_foralls(types, *ty))
                    .unwrap_or_default();
                let mut field_scope = scope.clone();
                open_child_binders(value, &binders, types, &mut field_scope, errors);
                verify_expr_scope(value, types, &mut field_scope, errors);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            verify_expr_scope(expression, types, scope, errors);
            for (_, value) in fields {
                verify_expr_scope(value, types, scope, errors);
            }
        }
        ExprKind::FieldAccess {
            expression: record, ..
        } => {
            let binders = leading_foralls(types, expression.ty);
            let mut record_scope = scope.clone();
            open_child_binders(record, &binders, types, &mut record_scope, errors);
            verify_expr_scope(record, types, &mut record_scope, errors)
        }
        ExprKind::Evidence(evidence) => verify_evidence_scope(evidence, types, scope, errors),
        ExprKind::Coerce {
            value,
            evidence,
            source_type,
            target_type,
        } => {
            verify_expr_scope(value, types, scope, errors);
            verify_evidence_scope(evidence, types, scope, errors);
            verify_type_scope(
                *source_type,
                types,
                scope,
                evidence.span,
                &mut HashSet::new(),
                errors,
            );
            verify_type_scope(
                *target_type,
                types,
                scope,
                evidence.span,
                &mut HashSet::new(),
                errors,
            );
        }
        ExprKind::UnsafeCoerce {
            value,
            source_type,
            target_type,
        } => {
            verify_expr_scope(value, types, scope, errors);
            verify_type_scope(
                *source_type,
                types,
                scope,
                expression.span,
                &mut HashSet::new(),
                errors,
            );
            verify_type_scope(
                *target_type,
                types,
                scope,
                expression.span,
                &mut HashSet::new(),
                errors,
            );
        }
        ExprKind::Application(function, argument) => {
            let binders = leading_foralls(types, expression.ty);
            let mut function_scope = scope.clone();
            open_child_binders(function, &binders, types, &mut function_scope, errors);
            verify_expr_scope(function, types, &mut function_scope, errors);
            let mut argument_scope = scope.clone();
            open_child_binders(argument, &binders, types, &mut argument_scope, errors);
            verify_expr_scope(argument, types, &mut argument_scope, errors);
        }
        ExprKind::Lambda { binder, body } => {
            let mut body_scope = scope.clone();
            open_expression_binders(expression, types, &mut body_scope, errors);
            verify_type_scope(
                binder.ty,
                types,
                &body_scope,
                binder.span,
                &mut HashSet::new(),
                errors,
            );
            verify_expr_scope(body, types, &mut body_scope, errors);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                let mut binding_scope = scope.clone();
                enter_binders(
                    &binding.quantified,
                    &mut binding_scope,
                    binding.span,
                    "binding quantifiers must be unique and lexically distinct",
                    errors,
                );
                verify_type_scope(
                    binding.binder.ty,
                    types,
                    &binding_scope,
                    binding.binder.span,
                    &mut HashSet::new(),
                    errors,
                );
                verify_expr_scope(&binding.value, types, &mut binding_scope, errors);
            }
            let mut body_scope = scope.clone();
            open_expression_binders(expression, types, &mut body_scope, errors);
            verify_expr_scope(body, types, &mut body_scope, errors);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            verify_expr_scope(condition, types, scope, errors);
            let mut branch_scope = scope.clone();
            open_expression_binders(expression, types, &mut branch_scope, errors);
            verify_expr_scope(then_branch, types, &mut branch_scope, errors);
            verify_expr_scope(else_branch, types, &mut branch_scope, errors);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            verify_expr_scope(scrutinee, types, scope, errors);
            let mut branch_scope = scope.clone();
            open_expression_binders(expression, types, &mut branch_scope, errors);
            for branch in branches {
                verify_pattern_scope(&branch.pattern, types, &branch_scope, errors);
                verify_expr_scope(&branch.value, types, &mut branch_scope, errors);
            }
        }
    }
}

fn open_expression_binders(
    expression: &Expr,
    types: &[Type],
    scope: &mut HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    let binders = leading_foralls(types, expression.ty);
    if binders.is_empty() {
        return;
    }
    let mut local = HashSet::new();
    if binders
        .iter()
        .any(|variable| !local.insert(*variable) || scope.contains(variable))
    {
        errors.push(VerifyError {
            span: expression.span,
            message: "forall binder shadows an active type variable",
        });
    }
    scope.extend(local);
}

fn verify_pattern_scope(
    pattern: &Pattern,
    types: &[Type],
    scope: &HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    verify_type_scope(
        pattern.ty,
        types,
        scope,
        pattern.span,
        &mut HashSet::new(),
        errors,
    );
    match &pattern.kind {
        PatternKind::Wildcard | PatternKind::Literal { .. } => {}
        PatternKind::Array { elements } => {
            for element in elements {
                verify_pattern_scope(element, types, scope, errors);
            }
        }
        PatternKind::Named { pattern, .. } => {
            verify_pattern_scope(pattern, types, scope, errors);
        }
        PatternKind::Var { ty, .. } => {
            verify_type_scope(*ty, types, scope, pattern.span, &mut HashSet::new(), errors)
        }
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                verify_pattern_scope(argument, types, scope, errors);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                verify_pattern_scope(field, types, scope, errors);
            }
        }
    }
}

fn verify_evidence_scope(
    evidence: &Evidence,
    types: &[Type],
    scope: &HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    verify_type_scope(
        evidence.ty,
        types,
        scope,
        evidence.span,
        &mut HashSet::new(),
        errors,
    );
    match &evidence.kind {
        EvidenceKind::Given(_) | EvidenceKind::Global(_) => {}
        EvidenceKind::Coercible {
            source_type,
            target_type,
        } => {
            verify_type_scope(
                *source_type,
                types,
                scope,
                evidence.span,
                &mut HashSet::new(),
                errors,
            );
            verify_type_scope(
                *target_type,
                types,
                scope,
                evidence.span,
                &mut HashSet::new(),
                errors,
            );
        }
        EvidenceKind::Superclass { parent, .. } => {
            verify_evidence_scope(parent, types, scope, errors)
        }
        EvidenceKind::Primitive { arguments } => {
            for argument in arguments {
                verify_type_scope(
                    *argument,
                    types,
                    scope,
                    evidence.span,
                    &mut HashSet::new(),
                    errors,
                );
            }
        }
        EvidenceKind::Instance {
            constructor_type,
            context,
            ..
        } => {
            verify_type_scope(
                *constructor_type,
                types,
                scope,
                evidence.span,
                &mut HashSet::new(),
                errors,
            );
            for argument in context {
                verify_evidence_scope(argument, types, scope, errors);
            }
        }
    }
}

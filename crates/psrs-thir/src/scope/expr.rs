use super::{
    enter_binders, leading_foralls, open_child_binders, open_expression_binders,
    verify_evidence_scope, verify_pattern_scope, verify_type_scope,
};
use crate::{Expr, ExprKind, Type, VerifyError};
use psrs_hir::TypeVariableId;
use std::collections::HashSet;

pub(super) fn verify_expr_scope(
    expression: &Expr,
    types: &[Type],
    scope: &mut HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    psrs_span::with_sufficient_stack(|| verify_expr_scope_inner(expression, types, scope, errors))
}

fn verify_expr_scope_inner(
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
            ..
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
            let mut body_scope = scope.clone();
            open_expression_binders(expression, types, &mut body_scope, errors);
            for binding in bindings {
                let mut binding_scope = body_scope.clone();
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

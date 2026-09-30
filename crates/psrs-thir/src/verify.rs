use crate::{
    Evidence, EvidenceKind, Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeId, VerifyError,
};
use psrs_span::TextRange;

pub(super) fn verify_module(module: &Module) -> Result<(), Vec<VerifyError>> {
    let mut errors = Vec::new();
    for ty in &module.types {
        match ty {
            Type::Application(parameter, result) => {
                verify_type_id(*parameter, module.types.len(), module.span, &mut errors);
                verify_type_id(*result, module.types.len(), module.span, &mut errors);
            }
            Type::RowExtend { ty, tail, .. } => {
                verify_type_id(*ty, module.types.len(), module.span, &mut errors);
                verify_type_id(*tail, module.types.len(), module.span, &mut errors);
            }
            _ => {}
        }
    }
    for constructor in &module.constructors {
        if module.opaque_ids.contains(&constructor.type_id) {
            errors.push(VerifyError {
                span: module.span,
                message: "an opaque type has no constructors",
            });
        }
    }
    for declaration in &module.declarations {
        verify_type_id(
            declaration.ty,
            module.types.len(),
            declaration.name_span,
            &mut errors,
        );
        verify_expr(&declaration.value, &module.types, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn verify_expr(expression: &Expr, types: &[Type], errors: &mut Vec<VerifyError>) {
    verify_type_id(expression.ty, types.len(), expression.span, errors);
    match &expression.kind {
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
        ExprKind::Array(elements) => {
            for element in elements {
                verify_expr(element, types, errors);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                verify_expr(value, types, errors);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            verify_expr(expression, types, errors);
            for (_, value) in fields {
                verify_expr(value, types, errors);
            }
        }
        ExprKind::FieldAccess { expression, .. } => verify_expr(expression, types, errors),
        ExprKind::Evidence(evidence) => verify_evidence(evidence, types, errors),
        ExprKind::Coerce {
            value,
            evidence,
            source_type,
            target_type,
        } => {
            verify_expr(value, types, errors);
            verify_evidence(evidence, types, errors);
            if evidence.class_id != psrs_hir::TypeId::COERCIBLE {
                errors.push(VerifyError {
                    span: evidence.span,
                    message: "coercion evidence does not prove Prim.Coerce.Coercible",
                });
            }
            if *source_type != value.ty || *target_type != expression.ty {
                errors.push(VerifyError {
                    span: evidence.span,
                    message: "coercion boundary types do not match its value and result",
                });
            }
            match &evidence.kind {
                EvidenceKind::Coercible {
                    source_type,
                    target_type,
                } if source_type == &value.ty && target_type == &expression.ty => {}
                EvidenceKind::Coercible { .. } => {
                    errors.push(VerifyError {
                        span: evidence.span,
                        message: "coercion evidence types do not match the cast boundary",
                    });
                }
                _ => errors.push(VerifyError {
                    span: evidence.span,
                    message: "coercion expression requires an explicit Coercible proof boundary",
                }),
            }
        }
        ExprKind::Application(function, argument) => {
            verify_expr(function, types, errors);
            verify_expr(argument, types, errors);
        }
        ExprKind::Lambda { binder, body } => {
            verify_type_id(binder.ty, types.len(), binder.span, errors);
            verify_expr(body, types, errors);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                verify_type_id(binding.binder.ty, types.len(), binding.binder.span, errors);
                verify_expr(&binding.value, types, errors);
            }
            verify_expr(body, types, errors);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            verify_expr(condition, types, errors);
            verify_expr(then_branch, types, errors);
            verify_expr(else_branch, types, errors);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            verify_expr(scrutinee, types, errors);
            for branch in branches {
                verify_pattern(&branch.pattern, types.len(), errors);
                verify_expr(&branch.value, types, errors);
            }
        }
    }
}

fn verify_evidence(evidence: &Evidence, types: &[Type], errors: &mut Vec<VerifyError>) {
    verify_type_id(evidence.ty, types.len(), evidence.span, errors);
    match &evidence.kind {
        EvidenceKind::Given(_) | EvidenceKind::Global(_) => {}
        EvidenceKind::Coercible {
            source_type,
            target_type,
        } => {
            verify_type_id(*source_type, types.len(), evidence.span, errors);
            verify_type_id(*target_type, types.len(), evidence.span, errors);
            if evidence.class_id != psrs_hir::TypeId::COERCIBLE {
                errors.push(VerifyError {
                    span: evidence.span,
                    message: "coercible evidence has the wrong class identity",
                });
            }
            let empty_dictionary = crate::record_row(types, evidence.ty)
                .and_then(|row| crate::row_fields(types, row))
                .is_some_and(|(fields, tail)| fields.is_empty() && tail.is_none());
            if !empty_dictionary {
                errors.push(VerifyError {
                    span: evidence.span,
                    message: "coercible evidence must have the empty class dictionary type",
                });
            }
        }
        EvidenceKind::Superclass { parent, field } => {
            verify_evidence(parent, types, errors);
            let Some(fields) = crate::record_fields(types, parent.ty) else {
                errors.push(VerifyError {
                    span: evidence.span,
                    message: "superclass evidence parent is not a dictionary record",
                });
                return;
            };
            match fields.iter().find(|(label, _)| label == field) {
                Some((_, field_ty)) if *field_ty == evidence.ty => {}
                _ => errors.push(VerifyError {
                    span: evidence.span,
                    message: "superclass evidence field has the wrong type",
                }),
            }
        }
        EvidenceKind::Instance {
            constructor_type,
            context,
            ..
        } => {
            verify_type_id(*constructor_type, types.len(), evidence.span, errors);
            let mut result = *constructor_type;
            for argument in context {
                verify_evidence(argument, types, errors);
                let Some((parameter, next)) = crate::arrow_parts(types, result) else {
                    errors.push(VerifyError {
                        span: argument.span,
                        message: "instance dictionary constructor takes too few context arguments",
                    });
                    return;
                };
                if parameter != argument.ty {
                    errors.push(VerifyError {
                        span: argument.span,
                        message: "instance evidence does not match its context parameter",
                    });
                }
                result = next;
            }
            if result != evidence.ty {
                errors.push(VerifyError {
                    span: evidence.span,
                    message: "instance evidence result has the wrong dictionary type",
                });
            }
        }
    }
}

fn verify_pattern(pattern: &Pattern, type_count: usize, errors: &mut Vec<VerifyError>) {
    verify_type_id(pattern.ty, type_count, pattern.span, errors);
    match &pattern.kind {
        PatternKind::Wildcard => {}
        PatternKind::Var { ty, .. } => verify_type_id(*ty, type_count, pattern.span, errors),
        PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                verify_pattern(argument, type_count, errors);
            }
        }
        PatternKind::Record { fields } => {
            for (_, field) in fields {
                verify_pattern(field, type_count, errors);
            }
        }
    }
}

fn verify_type_id(id: TypeId, type_count: usize, span: TextRange, errors: &mut Vec<VerifyError>) {
    if id.0 as usize >= type_count {
        errors.push(VerifyError {
            span,
            message: "type reference is outside the THIR type table",
        });
    }
}

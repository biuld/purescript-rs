use self::patterns::{is_shallow_pattern, match_pattern, substitute_case_bindings};
use super::effects;
use super::util::{FreshLocals, with_span};
use crate::{Binding, Expr, ExprKind, Module, Primitive};
use std::collections::HashMap;

mod patterns;

pub(super) fn run(mut module: Module) -> Module {
    let mut fresh = super::util::next_locals(&module);
    for (declaration, fresh) in module.declarations.iter_mut().zip(&mut fresh) {
        declaration.value = simplify_expr(declaration.value.clone(), fresh);
    }
    module
}

fn simplify_expr(mut expression: Expr, fresh: &mut FreshLocals) -> Expr {
    expression.kind = match expression.kind {
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol,
            arguments: arguments
                .into_iter()
                .map(|argument| simplify_expr(argument, fresh))
                .collect(),
        },
        ExprKind::Array { elements } => ExprKind::Array {
            elements: elements
                .into_iter()
                .map(|element| simplify_expr(element, fresh))
                .collect(),
        },
        ExprKind::Record { fields } => ExprKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, simplify_expr(value, fresh)))
                .collect(),
        },
        ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
            record: Box::new(simplify_expr(*record, fresh)),
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, simplify_expr(value, fresh)))
                .collect(),
        },
        ExprKind::FieldAccess { record, field } => {
            let record = simplify_expr(*record, fresh);
            if let ExprKind::Record { fields } = &record.kind
                && let Some(index) = fields.iter().position(|(label, _)| label == &field)
            {
                let values = fields.iter().map(|(_, value)| value.clone()).collect();
                if let Some(replacement) =
                    sequence_values(values, index, expression.ty, expression.span, fresh)
                {
                    return replacement;
                }
            }
            ExprKind::FieldAccess {
                record: Box::new(record),
                field,
            }
        }
        ExprKind::RepresentationCast {
            value,
            source_type,
            target_type,
        } => ExprKind::RepresentationCast {
            value: Box::new(simplify_expr(*value, fresh)),
            source_type,
            target_type,
        },
        // The string/byte conversions never fold to a literal: their result is
        // a heap value, and folding `bytesToString` would hide its validation.
        ExprKind::StringToBytes(value) => {
            ExprKind::StringToBytes(Box::new(simplify_expr(*value, fresh)))
        }
        ExprKind::BytesToString(value) => {
            ExprKind::BytesToString(Box::new(simplify_expr(*value, fresh)))
        }
        ExprKind::ArrayLength(array) => {
            let array = simplify_expr(*array, fresh);
            if let ExprKind::Array { elements } = &array.kind
                && let Ok(length) = i32::try_from(elements.len())
            {
                let values = elements.clone();
                if let Some(replacement) =
                    sequence_constant(values, length, expression.ty, expression.span, fresh)
                {
                    return replacement;
                }
            }
            ExprKind::ArrayLength(Box::new(array))
        }
        ExprKind::ArrayAppend { left, right } => ExprKind::ArrayAppend {
            left: Box::new(simplify_expr(*left, fresh)),
            right: Box::new(simplify_expr(*right, fresh)),
        },
        ExprKind::UnaryPrimitive { op, value } => ExprKind::UnaryPrimitive {
            op,
            value: Box::new(simplify_expr(*value, fresh)),
        },
        ExprKind::ArrayIndex { array, index } => {
            let array = simplify_expr(*array, fresh);
            let index = simplify_expr(*index, fresh);
            if let ExprKind::Array { elements } = &array.kind
                && let ExprKind::Integer(index_value) = &index.kind
                && let Ok(index_value) = usize::try_from(*index_value)
                && index_value < elements.len()
            {
                let values = elements.clone();
                if let Some(replacement) =
                    sequence_values(values, index_value, expression.ty, expression.span, fresh)
                {
                    return replacement;
                }
            }
            ExprKind::ArrayIndex {
                array: Box::new(array),
                index: Box::new(index),
            }
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => ExprKind::ArrayUpdate {
            array: Box::new(simplify_expr(*array, fresh)),
            index: Box::new(simplify_expr(*index, fresh)),
            value: Box::new(simplify_expr(*value, fresh)),
        },
        ExprKind::Primitive { op, left, right } => {
            let left = simplify_expr(*left, fresh);
            let right = simplify_expr(*right, fresh);
            if let Some(replacement) =
                fold_primitive(op, &left, &right, expression.ty, expression.span)
            {
                return replacement;
            }
            if let Some(replacement) = primitive_identity(op, &left, &right, expression.span) {
                return replacement;
            }
            ExprKind::Primitive {
                op,
                left: Box::new(left),
                right: Box::new(right),
            }
        }
        ExprKind::IntrinsicCall {
            intrinsic,
            arguments,
        } => {
            let arguments = arguments
                .into_iter()
                .map(|argument| simplify_expr(argument, fresh))
                .collect::<Vec<_>>();
            if let Some(op) = Primitive::from_intrinsic(intrinsic)
                && let [left, right] = arguments.as_slice()
            {
                if let Some(replacement) =
                    fold_primitive(op, left, right, expression.ty, expression.span)
                {
                    return replacement;
                }
                if let Some(replacement) = primitive_identity(op, left, right, expression.span) {
                    return replacement;
                }
            }
            if intrinsic == psrs_hir::Intrinsic::ArrayLength
                && let [array] = arguments.as_slice()
                && let ExprKind::Array { elements } = &array.kind
                && let Ok(length) = i32::try_from(elements.len())
                && let Some(replacement) = sequence_constant(
                    elements.clone(),
                    length,
                    expression.ty,
                    expression.span,
                    fresh,
                )
            {
                return replacement;
            }
            if intrinsic == psrs_hir::Intrinsic::ArrayIndex
                && let [array, index] = arguments.as_slice()
                && let ExprKind::Array { elements } = &array.kind
                && let ExprKind::Integer(index_value) = &index.kind
                && let Ok(index_value) = usize::try_from(*index_value)
                && index_value < elements.len()
                && let Some(replacement) = sequence_values(
                    elements.clone(),
                    index_value,
                    expression.ty,
                    expression.span,
                    fresh,
                )
            {
                return replacement;
            }
            ExprKind::IntrinsicCall {
                intrinsic,
                arguments,
            }
        }
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(simplify_expr(*function, fresh)),
            Box::new(simplify_expr(*argument, fresh)),
        ),
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder,
            body: Box::new(simplify_expr(*body, fresh)),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|mut binding| {
                    binding.value = simplify_expr(binding.value, fresh);
                    binding
                })
                .collect(),
            body: Box::new(simplify_expr(*body, fresh)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = simplify_expr(*condition, fresh);
            let then_branch = simplify_expr(*then_branch, fresh);
            let else_branch = simplify_expr(*else_branch, fresh);
            if let ExprKind::Boolean(value) = condition.kind {
                let mut replacement = if value { then_branch } else { else_branch };
                replacement.ty = expression.ty;
                replacement.span = expression.span;
                return replacement;
            }
            ExprKind::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            }
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            let scrutinee = simplify_expr(*scrutinee, fresh);
            let branches = branches
                .into_iter()
                .map(|mut branch| {
                    branch.value = simplify_expr(branch.value, fresh);
                    branch
                })
                .collect::<Vec<_>>();
            let known_shape = matches!(
                &scrutinee.kind,
                ExprKind::Constructor { .. } | ExprKind::Record { .. }
            );
            if known_shape && effects::summarize(&scrutinee).inert() {
                for branch in &branches {
                    // A nested constructor or record test may need a runtime
                    // cast for erased polymorphic fields. Core types alone do
                    // not prove that cast succeeds, so leave the whole case
                    // to P8 unless the top-level pattern binds or ignores each
                    // component without another test.
                    if !is_shallow_pattern(&branch.pattern) {
                        break;
                    }
                    let mut substitutions = HashMap::new();
                    if match_pattern(&branch.pattern, &scrutinee, &mut substitutions) {
                        let Some(value) = substitute_case_bindings(
                            &branch.value,
                            substitutions,
                            expression.ty,
                            expression.span,
                            fresh,
                        ) else {
                            break;
                        };
                        return value;
                    }
                }
            }
            ExprKind::Case {
                scrutinee: Box::new(scrutinee),
                branches,
            }
        }
        ExprKind::Local(id) => ExprKind::Local(id),
        ExprKind::Global(symbol) => ExprKind::Global(symbol),
        ExprKind::Integer(value) => ExprKind::Integer(value),
        ExprKind::Number(value) => ExprKind::Number(value),
        ExprKind::Boolean(value) => ExprKind::Boolean(value),
        ExprKind::String(value) => ExprKind::String(value),
        ExprKind::Char(value) => ExprKind::Char(value),
        kind @ (ExprKind::Unit | ExprKind::Trap) => kind,
    };
    expression
}

fn sequence_values(
    values: Vec<Expr>,
    selected: usize,
    result_type: crate::TypeId,
    span: psrs_span::TextRange,
    fresh: &mut FreshLocals,
) -> Option<Expr> {
    let mut bindings = Vec::with_capacity(values.len());
    let mut selected_id = None;
    for (index, value) in values.into_iter().enumerate() {
        let id = fresh.fresh()?;
        if index == selected {
            selected_id = Some(id);
        }
        bindings.push(Binding {
            binder: crate::Binder {
                id,
                name: format!("$p7_value_{index}"),
                ty: value.ty,
                span: value.span,
            },
            quantified: Vec::new(),
            span: value.span,
            value,
        });
    }
    let id = selected_id?;
    Some(Expr {
        kind: ExprKind::Let {
            bindings,
            body: Box::new(Expr {
                kind: ExprKind::Local(id),
                ty: result_type,
                span,
            }),
        },
        ty: result_type,
        span,
    })
}

fn sequence_constant(
    values: Vec<Expr>,
    constant: i32,
    result_type: crate::TypeId,
    span: psrs_span::TextRange,
    fresh: &mut FreshLocals,
) -> Option<Expr> {
    let mut bindings = Vec::with_capacity(values.len());
    for (index, value) in values.into_iter().enumerate() {
        let id = fresh.fresh()?;
        bindings.push(Binding {
            binder: crate::Binder {
                id,
                name: format!("$p7_value_{index}"),
                ty: value.ty,
                span: value.span,
            },
            quantified: Vec::new(),
            span: value.span,
            value,
        });
    }
    Some(Expr {
        kind: ExprKind::Let {
            bindings,
            body: Box::new(Expr {
                kind: ExprKind::Integer(constant),
                ty: result_type,
                span,
            }),
        },
        ty: result_type,
        span,
    })
}

fn fold_primitive(
    op: Primitive,
    left: &Expr,
    right: &Expr,
    result_type: crate::TypeId,
    span: psrs_span::TextRange,
) -> Option<Expr> {
    let (ExprKind::Integer(left), ExprKind::Integer(right)) = (&left.kind, &right.kind) else {
        return None;
    };
    let folded = match op {
        Primitive::IntAdd => ExprKind::Integer(left.wrapping_add(*right)),
        Primitive::IntSub => ExprKind::Integer(left.wrapping_sub(*right)),
        Primitive::IntMul => ExprKind::Integer(left.wrapping_mul(*right)),
        // checked_{div,rem} returns None for both trapping cases: zero divisor
        // and signed overflow. Leaving the operation intact preserves the trap.
        Primitive::IntQuot => ExprKind::Integer(left.checked_div(*right)?),
        Primitive::IntRem => ExprKind::Integer(left.checked_rem(*right)?),
        Primitive::IntEq => ExprKind::Boolean(left == right),
        Primitive::IntNe => ExprKind::Boolean(left != right),
        Primitive::IntLt => ExprKind::Boolean(left < right),
        Primitive::IntLe => ExprKind::Boolean(left <= right),
        Primitive::IntGt => ExprKind::Boolean(left > right),
        Primitive::IntGe => ExprKind::Boolean(left >= right),
        _ => return None,
    };
    Some(Expr {
        kind: folded,
        ty: result_type,
        span,
    })
}

fn primitive_identity(
    op: Primitive,
    left: &Expr,
    right: &Expr,
    span: psrs_span::TextRange,
) -> Option<Expr> {
    let zero = |ty| Expr {
        kind: ExprKind::Integer(0),
        ty,
        span,
    };
    match (op, &left.kind, &right.kind) {
        (Primitive::IntAdd, _, ExprKind::Integer(0))
        | (Primitive::IntSub, _, ExprKind::Integer(0))
        | (Primitive::IntMul, _, ExprKind::Integer(1)) => Some(with_span(left.clone(), span)),
        (Primitive::IntAdd, ExprKind::Integer(0), _)
        | (Primitive::IntMul, ExprKind::Integer(1), _) => Some(with_span(right.clone(), span)),
        (Primitive::IntMul, ExprKind::Integer(0), _) if effects::summarize(right).inert() => {
            Some(zero(left.ty))
        }
        (Primitive::IntMul, _, ExprKind::Integer(0)) if effects::summarize(left).inert() => {
            Some(zero(right.ty))
        }
        _ => None,
    }
}

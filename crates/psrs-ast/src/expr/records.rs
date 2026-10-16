use super::{Binder, Expr, ExprKind, RecordUpdateValue};
use crate::{LowerError, Name, lower_expr};
use psrs_cst::{self as cst, RecordField, RecordUpdateField};
use psrs_span::TextRange;

/// Only immediate anonymous arguments belong to this constructor. Nested
/// literals introduce their own lambdas; nested update paths share this one.
fn lower_argument(expression: cst::Expr, binders: &mut Vec<Binder>) -> Result<Expr, LowerError> {
    if matches!(&expression.kind, cst::ExprKind::Name(name) if name.text == "_") {
        let span = expression.span;
        let name = format!("$psrs_record_argument_{}", span.start);
        binders.push(Binder {
            name: name.clone(),
            span,
        });
        Ok(Expr {
            kind: ExprKind::Name(Name { text: name, span }),
            span,
        })
    } else {
        lower_expr(expression)
    }
}

fn wrap_arguments(kind: ExprKind, binders: Vec<Binder>, span: TextRange) -> Expr {
    let mut expression = Expr { kind, span };
    for binder in binders.into_iter().rev() {
        expression = Expr {
            kind: ExprKind::Lambda {
                binder,
                body: Box::new(expression),
            },
            span,
        };
    }
    expression
}

pub(crate) fn lower_record(
    fields: Vec<RecordField>,
    tail: Option<Box<cst::Expr>>,
    span: TextRange,
) -> Result<Expr, LowerError> {
    if tail.is_some() {
        return Err(LowerError::new(
            span,
            "open record rows are not supported yet",
        ));
    }
    let mut binders = Vec::new();
    let fields = fields
        .into_iter()
        .map(|field| Ok((field.label.text, lower_argument(field.value, &mut binders)?)))
        .collect::<Result<Vec<_>, LowerError>>()?;
    Ok(wrap_arguments(ExprKind::Record(fields), binders, span))
}

pub(crate) fn lower_record_update(
    expression: cst::Expr,
    fields: Vec<RecordUpdateField>,
    span: TextRange,
) -> Result<Expr, LowerError> {
    let mut binders = Vec::new();
    let expression = lower_argument(expression, &mut binders)?;
    let fields = lower_update_fields(fields, &mut binders)?;
    Ok(wrap_arguments(
        ExprKind::RecordUpdate {
            expression: Box::new(expression),
            fields,
        },
        binders,
        span,
    ))
}

fn lower_update_fields(
    fields: Vec<RecordUpdateField>,
    binders: &mut Vec<Binder>,
) -> Result<Vec<super::RecordUpdateField>, LowerError> {
    fields
        .into_iter()
        .map(|field| {
            // The parser marks path updates with the label span in place of
            // an equals token. Explicit `field = expression` is a new scope.
            let value = if field.equals_span == field.label.span {
                let span = field.value.span;
                let cst::ExprKind::RecordUpdate { fields, .. } = field.value.kind else {
                    return Err(LowerError::new(span, "invalid nested record update"));
                };
                RecordUpdateValue::Nested {
                    fields: lower_update_fields(fields, binders)?,
                    span,
                }
            } else {
                RecordUpdateValue::Expression(lower_argument(field.value, binders)?)
            };
            Ok(super::RecordUpdateField {
                label: field.label.text,
                value,
            })
        })
        .collect()
}

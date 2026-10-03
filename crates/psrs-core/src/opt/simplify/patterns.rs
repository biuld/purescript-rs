//! Pattern-based case folding for P7 simplification.
//!
//! A `case` whose scrutinee is a known constructor or record and whose first
//! matching branch is shallow can be replaced by that branch's body with its
//! bindings substituted. Anything that would need a runtime representation
//! test is left to P8.

use super::super::util::{FreshLocals, substitute_locals, with_span};
use crate::{Binding, Expr, ExprKind, Literal, Pattern, PatternKind};
use psrs_hir::LocalId;
use std::collections::HashMap;

pub(super) fn match_pattern(
    pattern: &Pattern,
    value: &Expr,
    substitutions: &mut HashMap<LocalId, Expr>,
) -> bool {
    match (&pattern.kind, &value.kind) {
        (PatternKind::Wildcard, _) => true,
        (PatternKind::Var { id, .. }, _) => {
            substitutions.insert(*id, value.clone());
            true
        }
        (PatternKind::Literal { value: literal }, kind) => match (literal, kind) {
            (Literal::Integer(pattern), ExprKind::Integer(value)) => pattern == value,
            (Literal::Number(pattern), ExprKind::Number(value)) => pattern
                .parse::<f64>()
                .ok()
                .zip(value.parse::<f64>().ok())
                .is_some_and(|(pattern, value)| pattern == value),
            (Literal::String(pattern), ExprKind::String(value)) => pattern == value,
            (Literal::Char(pattern), ExprKind::Char(value)) => pattern == value,
            (Literal::Boolean(pattern), ExprKind::Boolean(value)) => pattern == value,
            _ => false,
        },
        (PatternKind::Array { elements: patterns }, ExprKind::Array { elements: values }) => {
            patterns.len() == values.len()
                && patterns
                    .iter()
                    .zip(values)
                    .all(|(pattern, value)| match_pattern(pattern, value, substitutions))
        }
        (PatternKind::Named { id, pattern }, _) => {
            substitutions.insert(*id, value.clone());
            match_pattern(pattern, value, substitutions)
        }
        (
            PatternKind::Constructor {
                symbol: pattern_symbol,
                arguments: pattern_arguments,
            },
            ExprKind::Constructor {
                symbol: value_symbol,
                arguments: value_arguments,
            },
        ) if pattern_symbol == value_symbol && pattern_arguments.len() == value_arguments.len() => {
            pattern_arguments
                .iter()
                .zip(value_arguments)
                .all(|(pattern, value)| match_pattern(pattern, value, substitutions))
        }
        (PatternKind::Record { fields: patterns }, ExprKind::Record { fields: values }) => {
            patterns.iter().all(|(label, pattern)| {
                values
                    .iter()
                    .find(|(value_label, _)| value_label == label)
                    .is_some_and(|(_, value)| match_pattern(pattern, value, substitutions))
            })
        }
        _ => false,
    }
}

pub(super) fn substitute_case_bindings(
    branch: &Expr,
    substitutions: HashMap<LocalId, Expr>,
    result_type: crate::TypeId,
    span: psrs_span::TextRange,
    fresh: &mut FreshLocals,
) -> Option<Expr> {
    let mut substitutions = substitutions.into_iter().collect::<Vec<_>>();
    substitutions.sort_by_key(|(id, _)| id.0);
    let mut replacements = HashMap::with_capacity(substitutions.len());
    let mut bindings = Vec::with_capacity(substitutions.len());
    for (pattern_id, value) in substitutions {
        let id = fresh.fresh()?;
        replacements.insert(
            pattern_id,
            Expr {
                kind: ExprKind::Local(id),
                ty: value.ty,
                span,
            },
        );
        bindings.push(Binding {
            binder: crate::Binder {
                id,
                name: format!("$p7_case_{}", pattern_id.0),
                ty: value.ty,
                span: value.span,
            },
            quantified: Vec::new(),
            span: value.span,
            value,
        });
    }
    let body = substitute_locals(branch, &replacements);
    if bindings.is_empty() {
        return Some(with_span(body, span));
    }
    Some(Expr {
        kind: ExprKind::Let {
            bindings,
            body: Box::new(body),
        },
        ty: result_type,
        span,
    })
}

pub(super) fn is_shallow_pattern(pattern: &Pattern) -> bool {
    fn irrefutable(pattern: &Pattern) -> bool {
        match &pattern.kind {
            PatternKind::Wildcard | PatternKind::Var { .. } => true,
            PatternKind::Named { pattern, .. } => irrefutable(pattern),
            PatternKind::Literal { .. }
            | PatternKind::Array { .. }
            | PatternKind::Constructor { .. }
            | PatternKind::Record { .. } => false,
        }
    }
    match &pattern.kind {
        PatternKind::Wildcard | PatternKind::Var { .. } => true,
        PatternKind::Literal { .. } | PatternKind::Array { .. } => false,
        PatternKind::Named { pattern, .. } => irrefutable(pattern),
        PatternKind::Constructor { arguments, .. } => arguments.iter().all(irrefutable),
        PatternKind::Record { fields } => fields.iter().all(|(_, field)| irrefutable(field)),
    }
}

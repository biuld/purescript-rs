//! Renumbering of declaration-local variables while linking declarations.
//!
//! Linking concatenates the per-module value tables, so every type id and
//! quantified variable of a linked module shifts by the sizes of the modules
//! before it. Each expression kind is walked once, here.

use super::ExprKind;
use super::{shift_id, shift_variable};
use crate::{Binding, CaseBranch, Declaration, Expr, PatternKind};
use psrs_span::SourceId;

pub(super) fn shift_kind(
    kind: ExprKind,
    offset: u32,
    variable_offset: u32,
    source: psrs_span::SourceId,
) -> ExprKind {
    match kind {
        ExprKind::Local(id) => ExprKind::Local(id),
        ExprKind::Global(symbol) => ExprKind::Global(symbol),
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol,
            arguments: arguments
                .into_iter()
                .map(|argument| shift_expr(argument, offset, variable_offset, source))
                .collect(),
        },
        ExprKind::Integer(value) => ExprKind::Integer(value),
        ExprKind::Number(value) => ExprKind::Number(value),
        ExprKind::Boolean(value) => ExprKind::Boolean(value),
        ExprKind::String(value) => ExprKind::String(value),
        ExprKind::Char(value) => ExprKind::Char(value),
        ExprKind::Unit => ExprKind::Unit,
        ExprKind::Trap => ExprKind::Trap,
        ExprKind::Array { elements } => ExprKind::Array {
            elements: elements
                .into_iter()
                .map(|element| shift_expr(element, offset, variable_offset, source))
                .collect(),
        },
        ExprKind::Record { fields } => ExprKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, shift_expr(value, offset, variable_offset, source)))
                .collect(),
        },
        ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
            record: Box::new(shift_expr(*record, offset, variable_offset, source)),
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, shift_expr(value, offset, variable_offset, source)))
                .collect(),
        },
        ExprKind::FieldAccess { record, field } => ExprKind::FieldAccess {
            record: Box::new(shift_expr(*record, offset, variable_offset, source)),
            field,
        },
        ExprKind::IntrinsicCall {
            intrinsic,
            arguments,
        } => ExprKind::IntrinsicCall {
            intrinsic,
            arguments: arguments
                .into_iter()
                .map(|argument| shift_expr(argument, offset, variable_offset, source))
                .collect(),
        },
        ExprKind::RepresentationCast {
            value,
            source_type,
            target_type,
        } => ExprKind::RepresentationCast {
            value: Box::new(shift_expr(*value, offset, variable_offset, source)),
            source_type: super::shift_id(source_type, offset),
            target_type: super::shift_id(target_type, offset),
        },
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(shift_expr(*function, offset, variable_offset, source)),
            Box::new(shift_expr(*argument, offset, variable_offset, source)),
        ),
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder: crate::Binder {
                id: binder.id,
                name: binder.name,
                ty: super::shift_id(binder.ty, offset),
                span: binder.span.in_source(source),
            },
            body: Box::new(shift_expr(*body, offset, variable_offset, source)),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|binding| shift_binding(binding, offset, variable_offset, source))
                .collect(),
            body: Box::new(shift_expr(*body, offset, variable_offset, source)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(shift_expr(*condition, offset, variable_offset, source)),
            then_branch: Box::new(shift_expr(*then_branch, offset, variable_offset, source)),
            else_branch: Box::new(shift_expr(*else_branch, offset, variable_offset, source)),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(shift_expr(*scrutinee, offset, variable_offset, source)),
            branches: branches
                .into_iter()
                .map(|branch| CaseBranch {
                    pattern: shift_pattern(branch.pattern, offset, source),
                    value: shift_expr(branch.value, offset, variable_offset, source),
                    span: branch.span.in_source(source),
                    coverage: branch.coverage,
                })
                .collect(),
        },
    }
}

fn shift_binding(binding: Binding, offset: u32, variable_offset: u32, source: SourceId) -> Binding {
    Binding {
        binder: crate::Binder {
            id: binding.binder.id,
            name: binding.binder.name,
            ty: shift_id(binding.binder.ty, offset),
            span: binding.binder.span.in_source(source),
        },
        quantified: binding
            .quantified
            .into_iter()
            .map(|variable| shift_variable(variable, variable_offset))
            .collect(),
        value: shift_expr(binding.value, offset, variable_offset, source),
        span: binding.span.in_source(source),
    }
}

pub(super) fn shift_declaration(
    declaration: Declaration,
    offset: u32,
    variable_offset: u32,
    source: SourceId,
) -> Declaration {
    Declaration {
        symbol: declaration.symbol,
        name: declaration.name,
        name_span: declaration.name_span.in_source(source),
        quantified: declaration
            .quantified
            .into_iter()
            .map(|variable| shift_variable(variable, variable_offset))
            .collect(),
        ty: shift_id(declaration.ty, offset),
        value: shift_expr(declaration.value, offset, variable_offset, source),
        span: declaration.span.in_source(source),
    }
}

fn shift_expr(expression: Expr, offset: u32, variable_offset: u32, source: SourceId) -> Expr {
    psrs_span::with_sufficient_stack(|| {
        shift_expr_inner(expression, offset, variable_offset, source)
    })
}

fn shift_expr_inner(expression: Expr, offset: u32, variable_offset: u32, source: SourceId) -> Expr {
    Expr {
        kind: shift_kind(expression.kind, offset, variable_offset, source),
        ty: shift_id(expression.ty, offset),
        span: expression.span.in_source(source),
    }
}

fn shift_pattern(pattern: crate::Pattern, offset: u32, source: SourceId) -> crate::Pattern {
    let kind = match pattern.kind {
        PatternKind::Wildcard => PatternKind::Wildcard,
        PatternKind::Literal { value } => PatternKind::Literal { value },
        PatternKind::Array { elements } => PatternKind::Array {
            elements: elements
                .into_iter()
                .map(|element| shift_pattern(element, offset, source))
                .collect(),
        },
        PatternKind::Named { id, pattern } => PatternKind::Named {
            id,
            pattern: Box::new(shift_pattern(*pattern, offset, source)),
        },
        PatternKind::Var { id, ty } => PatternKind::Var {
            id,
            ty: shift_id(ty, offset),
        },
        PatternKind::Constructor { symbol, arguments } => PatternKind::Constructor {
            symbol,
            arguments: arguments
                .into_iter()
                .map(|argument| shift_pattern(argument, offset, source))
                .collect(),
        },
        PatternKind::Record { fields } => PatternKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, pattern)| (label, shift_pattern(pattern, offset, source)))
                .collect(),
        },
    };
    crate::Pattern {
        kind,
        ty: shift_id(pattern.ty, offset),
        span: pattern.span.in_source(source),
    }
}

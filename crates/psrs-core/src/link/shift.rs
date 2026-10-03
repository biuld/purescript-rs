//! Renumbering of declaration-local variables while linking declarations.
//!
//! Linking concatenates the per-module value tables, so every type id and
//! quantified variable of a linked module shifts by the sizes of the modules
//! before it. Each expression kind is walked once, here.

use super::ExprKind;
use crate::CaseBranch;

pub(super) fn shift_kind(kind: ExprKind, offset: u32, variable_offset: u32) -> ExprKind {
    match kind {
        ExprKind::Local(id) => ExprKind::Local(id),
        ExprKind::Global(symbol) => ExprKind::Global(symbol),
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol,
            arguments: arguments
                .into_iter()
                .map(|argument| super::shift_expr(argument, offset, variable_offset))
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
                .map(|element| super::shift_expr(element, offset, variable_offset))
                .collect(),
        },
        ExprKind::Record { fields } => ExprKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, super::shift_expr(value, offset, variable_offset)))
                .collect(),
        },
        ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
            record: Box::new(super::shift_expr(*record, offset, variable_offset)),
            fields: fields
                .into_iter()
                .map(|(label, value)| (label, super::shift_expr(value, offset, variable_offset)))
                .collect(),
        },
        ExprKind::FieldAccess { record, field } => ExprKind::FieldAccess {
            record: Box::new(super::shift_expr(*record, offset, variable_offset)),
            field,
        },
        ExprKind::RepresentationCast {
            value,
            source_type,
            target_type,
        } => ExprKind::RepresentationCast {
            value: Box::new(super::shift_expr(*value, offset, variable_offset)),
            source_type: super::shift_id(source_type, offset),
            target_type: super::shift_id(target_type, offset),
        },
        ExprKind::StringToBytes(value) => {
            ExprKind::StringToBytes(Box::new(super::shift_expr(*value, offset, variable_offset)))
        }
        ExprKind::BytesToString(value) => {
            ExprKind::BytesToString(Box::new(super::shift_expr(*value, offset, variable_offset)))
        }
        ExprKind::ArrayLength(value) => {
            ExprKind::ArrayLength(Box::new(super::shift_expr(*value, offset, variable_offset)))
        }
        ExprKind::ArrayIndex { array, index } => ExprKind::ArrayIndex {
            array: Box::new(super::shift_expr(*array, offset, variable_offset)),
            index: Box::new(super::shift_expr(*index, offset, variable_offset)),
        },
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => ExprKind::ArrayUpdate {
            array: Box::new(super::shift_expr(*array, offset, variable_offset)),
            index: Box::new(super::shift_expr(*index, offset, variable_offset)),
            value: Box::new(super::shift_expr(*value, offset, variable_offset)),
        },
        ExprKind::Primitive { op, left, right } => ExprKind::Primitive {
            op,
            left: Box::new(super::shift_expr(*left, offset, variable_offset)),
            right: Box::new(super::shift_expr(*right, offset, variable_offset)),
        },
        ExprKind::UnaryPrimitive { op, value } => ExprKind::UnaryPrimitive {
            op,
            value: Box::new(super::shift_expr(*value, offset, variable_offset)),
        },
        ExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(super::shift_expr(*function, offset, variable_offset)),
            Box::new(super::shift_expr(*argument, offset, variable_offset)),
        ),
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder: crate::Binder {
                id: binder.id,
                name: binder.name,
                ty: super::shift_id(binder.ty, offset),
                span: binder.span,
            },
            body: Box::new(super::shift_expr(*body, offset, variable_offset)),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|binding| super::shift_binding(binding, offset, variable_offset))
                .collect(),
            body: Box::new(super::shift_expr(*body, offset, variable_offset)),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(super::shift_expr(*condition, offset, variable_offset)),
            then_branch: Box::new(super::shift_expr(*then_branch, offset, variable_offset)),
            else_branch: Box::new(super::shift_expr(*else_branch, offset, variable_offset)),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(super::shift_expr(*scrutinee, offset, variable_offset)),
            branches: branches
                .into_iter()
                .map(|branch| CaseBranch {
                    pattern: super::shift_pattern(branch.pattern, offset),
                    value: super::shift_expr(branch.value, offset, variable_offset),
                    span: branch.span,
                    coverage: branch.coverage,
                })
                .collect(),
        },
    }
}

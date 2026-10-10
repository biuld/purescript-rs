//! Identifier supplies used while representation lowering adds declarations.

use super::Module;
use crate::{Expr, ExprKind, Type};
use psrs_hir::{LocalId, TypeVariableId};

/// A local id above every binder the module already holds.
pub(super) struct LocalSupply {
    next: u32,
}

impl LocalSupply {
    pub(super) fn new(module: &Module) -> Self {
        let mut next = 0;
        for declaration in &module.declarations {
            next = next.max(max_local(&declaration.value) + 1);
        }
        Self { next }
    }

    pub(super) fn fresh(&mut self) -> LocalId {
        let id = LocalId(self.next);
        self.next += 1;
        id
    }
}

fn max_local(expression: &Expr) -> u32 {
    match &expression.kind {
        ExprKind::Local(id) => id.0,
        ExprKind::Lambda { binder, body } => binder.id.0.max(max_local(body)),
        ExprKind::Application(function, argument) => max_local(function).max(max_local(argument)),
        ExprKind::Let { bindings, body } => bindings
            .iter()
            .map(|binding| binding.binder.id.0.max(max_local(&binding.value)))
            .chain(std::iter::once(max_local(body)))
            .max()
            .unwrap_or(0),
        ExprKind::Case {
            scrutinee,
            branches,
        } => branches
            .iter()
            .map(|branch| max_local(&branch.value))
            .chain(std::iter::once(max_local(scrutinee)))
            .max()
            .unwrap_or(0),
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => max_local(condition)
            .max(max_local(then_branch))
            .max(max_local(else_branch)),
        ExprKind::Constructor { arguments, .. }
        | ExprKind::IntrinsicCall { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => arguments.iter().map(max_local).max().unwrap_or(0),
        ExprKind::Record { fields } => fields
            .iter()
            .map(|(_, value)| max_local(value))
            .max()
            .unwrap_or(0),
        ExprKind::RecordUpdate { record, fields } => fields
            .iter()
            .map(|(_, value)| max_local(value))
            .chain(std::iter::once(max_local(record)))
            .max()
            .unwrap_or(0),
        ExprKind::FieldAccess { record, .. }
        | ExprKind::RepresentationCast { value: record, .. } => max_local(record),
        ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => 0,
        ExprKind::Unit | ExprKind::StateToken | ExprKind::Trap => 0,
    }
}

/// A type variable above every binder the module already holds.
pub(super) struct VariableSupply {
    next: u32,
}

impl VariableSupply {
    pub(super) fn new(module: &Module) -> Self {
        let mut next = 0;
        for ty in &module.types {
            match ty {
                Type::Variable(variable) => next = next.max(variable.0 + 1),
                Type::ForAll { variables, .. } => {
                    for variable in variables {
                        next = next.max(variable.0 + 1);
                    }
                }
                _ => {}
            }
        }
        for declaration in &module.declarations {
            for variable in &declaration.quantified {
                next = next.max(variable.0 + 1);
            }
        }
        Self { next }
    }

    pub(super) fn fresh(&mut self) -> TypeVariableId {
        let id = TypeVariableId(self.next);
        self.next += 1;
        id
    }
}

//! A deliberately small proof of an ordinary body's state pass-through.
//! Unknown/higher-order invocations remain observable, without name exceptions.
use crate::{Expr, ExprKind, Module};

pub(super) fn passthrough(module: &Module, callable: &Expr) -> bool {
    let mut callable = callable;
    loop {
        match &callable.kind {
            ExprKind::Application(function, _) => callable = function,
            ExprKind::Global(symbol) => {
                let Some(declaration) = module
                    .declarations
                    .iter()
                    .find(|value| value.symbol == *symbol)
                else {
                    return false;
                };
                return lambda(module, &declaration.value);
            }
            ExprKind::Lambda { .. } => return lambda(module, callable),
            _ => return false,
        }
    }
}

fn lambda(module: &Module, mut callable: &Expr) -> bool {
    while let ExprKind::Lambda { binder, body } = &callable.kind {
        if crate::state::region(module, binder.ty).is_some() {
            return crate::state::step(module, body.ty).is_some() && inert(body);
        }
        callable = body;
    }
    false
}

fn inert(value: &Expr) -> bool {
    psrs_span::with_sufficient_stack(|| match &value.kind {
        ExprKind::Local(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Unit
        | ExprKind::Lambda { .. } => true,
        ExprKind::Record { fields } => fields.iter().all(|(_, value)| inert(value)),
        ExprKind::FieldAccess { record, .. } => inert(record),
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => arguments.iter().all(inert),
        ExprKind::Let { bindings, body } => {
            bindings.iter().all(|binding| inert(&binding.value)) && inert(body)
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => inert(condition) && inert(then_branch) && inert(else_branch),
        ExprKind::Case {
            scrutinee,
            branches,
        } => inert(scrutinee) && branches.iter().all(|branch| inert(&branch.value)),
        _ => false,
    })
}

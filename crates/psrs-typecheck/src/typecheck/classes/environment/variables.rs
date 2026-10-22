//! Type-variable names collected from class and instance types.

use super::super::super::*;

/// The set of type-variable names used anywhere in a class or context type.
pub(super) fn collect_variables(ty: &hir::Type, out: &mut Vec<String>) {
    match &ty.kind {
        hir::TypeKind::Variable(name) => out.push(name.clone()),
        hir::TypeKind::Application(function, argument) => {
            collect_variables(function, out);
            collect_variables(argument, out);
        }
        hir::TypeKind::OperatorChain { operands, .. } => {
            for operand in operands {
                collect_variables(operand, out);
            }
        }
        hir::TypeKind::Function { parameter, result } => {
            collect_variables(parameter, out);
            collect_variables(result, out);
        }
        hir::TypeKind::Record { fields, tail } | hir::TypeKind::Row { fields, tail } => {
            for field in fields {
                collect_variables(&field.ty, out);
            }
            if let Some(tail) = tail {
                collect_variables(tail, out);
            }
        }
        hir::TypeKind::Kinded { expression, kind } => {
            collect_variables(expression, out);
            collect_variables(kind, out);
        }
        hir::TypeKind::Forall { body, .. } => collect_variables(body, out),
        hir::TypeKind::Constrained { constraint, body } => {
            collect_variables(constraint, out);
            collect_variables(body, out);
        }
        hir::TypeKind::Wildcard
        | hir::TypeKind::Hole(_)
        | hir::TypeKind::Constructor(_)
        | hir::TypeKind::Named(_)
        | hir::TypeKind::Opaque(_)
        | hir::TypeKind::Integer(_)
        | hir::TypeKind::String(_) => {}
    }
}

/// The type-variable names appearing in an instance head's arguments.
pub(super) fn head_variables(arguments: &[&hir::Type]) -> Vec<String> {
    let mut names = Vec::new();
    for argument in arguments {
        collect_variables(argument, &mut names);
    }
    names
}

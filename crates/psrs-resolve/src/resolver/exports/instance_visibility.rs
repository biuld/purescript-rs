use super::*;
use std::collections::HashSet;

pub(super) fn instance_is_public(
    instance: &hir::InstanceDeclaration,
    module_id: ModuleId,
    exported_types: &HashSet<TypeId>,
) -> bool {
    let mut referenced_types = vec![instance.class_id];
    for constraint in &instance.context {
        collect_named_type_ids(constraint, &mut referenced_types);
    }
    collect_named_type_ids(&instance.head, &mut referenced_types);
    referenced_types
        .into_iter()
        .all(|id| id.module != module_id || exported_types.contains(&id))
}

fn collect_named_type_ids(ty: &hir::Type, out: &mut Vec<TypeId>) {
    match &ty.kind {
        hir::TypeKind::Named(id) | hir::TypeKind::Opaque(id) => out.push(*id),
        hir::TypeKind::Application(function, argument) => {
            collect_named_type_ids(function, out);
            collect_named_type_ids(argument, out);
        }
        hir::TypeKind::OperatorChain {
            operands,
            operators,
        } => {
            for operator in operators {
                match operator.head {
                    hir::ResolvedTypeHead::Named(id) | hir::ResolvedTypeHead::Opaque(id) => {
                        out.push(id);
                    }
                    hir::ResolvedTypeHead::Builtin(_) => {}
                }
            }
            for operand in operands {
                collect_named_type_ids(operand, out);
            }
        }
        hir::TypeKind::Function { parameter, result } => {
            collect_named_type_ids(parameter, out);
            collect_named_type_ids(result, out);
        }
        hir::TypeKind::Forall { variables, body } => {
            for variable in variables {
                if let Some(kind) = &variable.kind {
                    collect_named_type_ids(kind, out);
                }
            }
            collect_named_type_ids(body, out);
        }
        hir::TypeKind::Constrained { constraint, body } => {
            collect_named_type_ids(constraint, out);
            collect_named_type_ids(body, out);
        }
        hir::TypeKind::Row { fields, tail } | hir::TypeKind::Record { fields, tail } => {
            for field in fields {
                collect_named_type_ids(&field.ty, out);
            }
            if let Some(tail) = tail {
                collect_named_type_ids(tail, out);
            }
        }
        hir::TypeKind::Wildcard
        | hir::TypeKind::Variable(_)
        | hir::TypeKind::Constructor(_)
        | hir::TypeKind::Integer(_)
        | hir::TypeKind::String(_) => {}
    }
}

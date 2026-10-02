use super::super::super::*;

/// Checks that free variables in a method signature belong to the class.
pub(super) fn validate_method_signature(
    signature: &hir::Type,
    parameters: &[String],
) -> Result<(), String> {
    let mut variables = Vec::new();
    collect_signature_variables(signature, &mut Vec::new(), &mut variables)?;
    for variable in variables {
        if !parameters.contains(&variable) {
            return Err(format!(
                "class method signature uses `{variable}`, which is not a class parameter"
            ));
        }
    }
    Ok(())
}

fn collect_signature_variables(
    ty: &hir::Type,
    bound: &mut Vec<String>,
    out: &mut Vec<String>,
) -> Result<(), String> {
    match &ty.kind {
        hir::TypeKind::Variable(name) => {
            if !bound.contains(name) {
                out.push(name.clone());
            }
        }
        hir::TypeKind::Constrained { constraint, body } => {
            collect_signature_variables(constraint, bound, out)?;
            collect_signature_variables(body, bound, out)?;
        }
        hir::TypeKind::Forall { variables, body } => {
            let old_len = bound.len();
            bound.extend(variables.iter().map(|variable| variable.name.clone()));
            let result = collect_signature_variables(body, bound, out);
            bound.truncate(old_len);
            return result;
        }
        hir::TypeKind::Application(function, argument) => {
            collect_signature_variables(function, bound, out)?;
            collect_signature_variables(argument, bound, out)?;
        }
        hir::TypeKind::OperatorChain { operands, .. } => {
            for operand in operands {
                collect_signature_variables(operand, bound, out)?;
            }
        }
        hir::TypeKind::Function { parameter, result } => {
            collect_signature_variables(parameter, bound, out)?;
            collect_signature_variables(result, bound, out)?;
        }
        hir::TypeKind::Record { fields, tail } | hir::TypeKind::Row { fields, tail } => {
            for field in fields {
                collect_signature_variables(&field.ty, bound, out)?;
            }
            if let Some(tail) = tail {
                collect_signature_variables(tail, bound, out)?;
            }
        }
        hir::TypeKind::Constructor(_)
        | hir::TypeKind::Named(_)
        | hir::TypeKind::Opaque(_)
        | hir::TypeKind::Integer(_)
        | hir::TypeKind::String(_) => {}
    }
    Ok(())
}

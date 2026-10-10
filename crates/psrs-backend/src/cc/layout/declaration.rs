//! The syntactic calling boundary of a checked Core declaration.

use crate::BackendError;
use psrs_core::{Declaration, ExprKind, Module, TypeId};
use psrs_span::TextRange;

pub(in crate::cc) struct DeclarationCall {
    pub parameters: Vec<(TypeId, TextRange)>,
    pub result: TypeId,
}

/// Only the leading lambdas belong to this declaration's call. A case, let,
/// alias, quantified result or explicit closure result produces a value whose
/// own parameters must be called separately.
pub(in crate::cc) fn declaration_call_parts(
    module: &Module,
    declaration: &Declaration,
) -> Result<DeclarationCall, Vec<BackendError>> {
    let mut ty = super::unquantified_type(module, declaration.ty);
    let mut value = &declaration.value;
    let mut parameters = Vec::new();
    if let Some((closure_parameters, result)) = psrs_core::closure_parts(&module.types, ty) {
        let mut peeled = 0;
        for parameter in closure_parameters {
            let ExprKind::Lambda { binder, body } = &value.kind else {
                break;
            };
            if !module.types_equivalent(binder.ty, *parameter) {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    binder.span,
                    "lambda binder type differs from the function parameter type",
                )]);
            }
            parameters.push((binder.ty, binder.span));
            value = body;
            peeled += 1;
        }
        if peeled == closure_parameters.len() {
            ty = result;
        } else if peeled != 0 {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                declaration.span,
                "closure declaration is missing a parameter",
            )]);
        }
    }
    while let ExprKind::Lambda { binder, body } = &value.kind {
        if psrs_core::closure_parts(&module.types, ty).is_some() {
            break;
        }
        let Some((parameter, result)) = psrs_core::arrow_parts(&module.types, ty) else {
            break;
        };
        if !module.types_equivalent(parameter, binder.ty) {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                binder.span,
                "lambda binder type differs from the function parameter type",
            )]);
        }
        parameters.push((binder.ty, binder.span));
        ty = result;
        value = body;
        if psrs_core::forall_parts(&module.types, ty).is_some() {
            break;
        }
    }
    Ok(DeclarationCall {
        parameters,
        result: ty,
    })
}

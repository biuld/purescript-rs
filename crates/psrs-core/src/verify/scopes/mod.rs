use crate::{Module, TypeId, VerifyError};
use psrs_hir::TypeVariableId;
use psrs_span::TextRange;
use std::collections::HashSet;

mod expr;
mod types;

pub(super) fn verify_module(module: &Module) -> Vec<VerifyError> {
    let mut errors = Vec::new();
    let mut complete = HashSet::new();
    for index in 0..module.types.len() {
        types::acyclic(
            TypeId(index as u32),
            module,
            &mut HashSet::new(),
            &mut complete,
            module.span,
            &mut errors,
        );
    }
    for constructor in &module.constructors {
        let mut scope = HashSet::new();
        enter(
            &constructor.parameters,
            &mut scope,
            module.id,
            module.span,
            "constructor type parameters must be unique",
            &mut errors,
        );
        for field in &constructor.field_types {
            types::scoped_type(
                *field,
                module,
                &scope,
                module.span,
                &mut HashSet::new(),
                &mut errors,
            );
        }
    }
    for declaration in &module.declarations {
        let mut scope = HashSet::new();
        enter(
            &declaration.quantified,
            &mut scope,
            module.id,
            declaration.name_span,
            "declaration quantifiers must be unique",
            &mut errors,
        );
        types::scoped_type(
            declaration.ty,
            module,
            &scope,
            declaration.name_span,
            &mut HashSet::new(),
            &mut errors,
        );
        expr::scoped_expr(&declaration.value, module, &mut scope, &mut errors);
    }
    errors
}

pub(super) fn enter(
    variables: &[TypeVariableId],
    scope: &mut HashSet<TypeVariableId>,
    module: psrs_hir::ModuleId,
    span: TextRange,
    message: &'static str,
    errors: &mut Vec<VerifyError>,
) {
    let mut local = HashSet::new();
    if variables
        .iter()
        .any(|variable| !local.insert(*variable) || scope.contains(variable))
    {
        errors.push(VerifyError {
            module,
            span,
            message,
        });
    }
    scope.extend(local);
}

pub(super) fn leading_foralls(module: &Module, mut id: TypeId) -> Vec<TypeVariableId> {
    let mut binders = Vec::new();
    let mut seen = HashSet::new();
    while let Some(crate::Type::ForAll { variables, body }) = module.types.get(id.0 as usize) {
        if !seen.insert(id) {
            break;
        }
        binders.extend(variables.iter().copied());
        id = *body;
    }
    binders
}

pub(super) fn error(module: &Module, span: TextRange, message: &'static str) -> VerifyError {
    VerifyError {
        module: module.id,
        span,
        message,
    }
}

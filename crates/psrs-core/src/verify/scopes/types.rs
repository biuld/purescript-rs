use super::{enter, error};
use crate::{Module, Type, TypeId, VerifyError};
use psrs_hir::TypeVariableId;
use psrs_span::TextRange;
use std::collections::HashSet;

pub(super) fn acyclic(
    id: TypeId,
    module: &Module,
    active: &mut HashSet<TypeId>,
    complete: &mut HashSet<TypeId>,
    span: TextRange,
    errors: &mut Vec<VerifyError>,
) {
    if complete.contains(&id) {
        return;
    }
    if !active.insert(id) {
        errors.push(error(module, span, "type table contains a cycle"));
        return;
    }
    match module.types.get(id.0 as usize) {
        Some(Type::Application(function, argument)) => {
            acyclic(*function, module, active, complete, span, errors);
            acyclic(*argument, module, active, complete, span, errors);
        }
        Some(Type::ForAll { body, .. }) => acyclic(*body, module, active, complete, span, errors),
        Some(Type::RowExtend { ty, tail, .. }) => {
            acyclic(*ty, module, active, complete, span, errors);
            acyclic(*tail, module, active, complete, span, errors);
        }
        Some(Type::Closure { parameters, result }) => {
            for parameter in parameters {
                acyclic(*parameter, module, active, complete, span, errors);
            }
            acyclic(*result, module, active, complete, span, errors);
        }
        _ => {}
    }
    active.remove(&id);
    complete.insert(id);
}

pub(super) fn scoped_type(
    id: TypeId,
    module: &Module,
    scope: &HashSet<TypeVariableId>,
    span: TextRange,
    active: &mut HashSet<TypeId>,
    errors: &mut Vec<VerifyError>,
) {
    if !active.insert(id) {
        return;
    }
    match module.types.get(id.0 as usize) {
        Some(Type::Variable(variable)) if !scope.contains(variable) => {
            errors.push(error(
                module,
                span,
                "type variable is outside its quantifier scope",
            ));
        }
        Some(Type::Application(function, argument)) => {
            scoped_type(*function, module, scope, span, active, errors);
            scoped_type(*argument, module, scope, span, active, errors);
        }
        Some(Type::ForAll { variables, body }) => {
            let mut nested = scope.clone();
            if variables.is_empty() {
                errors.push(error(
                    module,
                    span,
                    "forall binders must be non-empty, unique, and lexically distinct",
                ));
            }
            enter(
                variables,
                &mut nested,
                module.id,
                span,
                "forall binders must be non-empty, unique, and lexically distinct",
                errors,
            );
            scoped_type(*body, module, &nested, span, active, errors);
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            scoped_type(*ty, module, scope, span, active, errors);
            scoped_type(*tail, module, scope, span, active, errors);
        }
        Some(Type::Closure { parameters, result }) => {
            for parameter in parameters {
                scoped_type(*parameter, module, scope, span, active, errors);
            }
            scoped_type(*result, module, scope, span, active, errors);
        }
        _ => {}
    }
    active.remove(&id);
}

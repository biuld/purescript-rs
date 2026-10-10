use super::free_type_variables::free_type_variables;
use crate::{Expr, Type, TypeId, VerifyError};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

pub(super) fn open_child_binders(
    child: &Expr,
    candidates: &[TypeVariableId],
    types: &[Type],
    scope: &mut HashSet<TypeVariableId>,
    errors: &mut Vec<VerifyError>,
) {
    let mut free = HashSet::new();
    free_type_variables(
        child.ty,
        types,
        &mut HashMap::new(),
        &mut HashSet::new(),
        &mut free,
    );
    let relevant = candidates
        .iter()
        .copied()
        .filter(|variable| free.contains(variable))
        .collect::<Vec<_>>();
    if relevant.is_empty() {
        return;
    }
    let mut local = HashSet::new();
    if relevant
        .iter()
        .any(|variable| !local.insert(*variable) || scope.contains(variable))
    {
        errors.push(VerifyError {
            span: child.span,
            message: "forall binder shadows an active type variable",
        });
    }
    scope.extend(local);
}

pub(super) fn leading_foralls(types: &[Type], mut id: TypeId) -> Vec<TypeVariableId> {
    let mut variables = Vec::new();
    let mut seen = HashSet::new();
    while let Some(Type::ForAll {
        variables: binders,
        body,
    }) = types.get(id.0 as usize)
    {
        if !seen.insert(id) {
            break;
        }
        variables.extend(binders.iter().copied());
        id = *body;
    }
    variables
}

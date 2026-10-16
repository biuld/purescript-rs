use crate::{Module, Type, TypeId};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

/// Free variables of a type in a verified module, respecting lexical forall
/// scope. Declaration quantifiers may also bind variables used only in bodies.
pub(crate) fn free_variables(module: &Module, id: TypeId) -> HashSet<TypeVariableId> {
    let mut free = HashSet::new();
    collect_free_variables(
        id,
        &module.types,
        &mut HashMap::new(),
        &mut HashSet::new(),
        &mut free,
    );
    free
}

pub(crate) fn collect_free_variables(
    id: TypeId,
    types: &[Type],
    bound: &mut HashMap<TypeVariableId, usize>,
    active: &mut HashSet<TypeId>,
    free: &mut HashSet<TypeVariableId>,
) {
    if !active.insert(id) {
        return;
    }
    match types.get(id.0 as usize) {
        Some(Type::Variable(variable)) if !bound.contains_key(variable) => {
            free.insert(*variable);
        }
        Some(Type::Application(function, argument)) => {
            collect_free_variables(*function, types, bound, active, free);
            collect_free_variables(*argument, types, bound, active, free);
        }
        Some(Type::ForAll { variables, body }) => {
            for variable in variables {
                *bound.entry(*variable).or_default() += 1;
            }
            collect_free_variables(*body, types, bound, active, free);
            for variable in variables {
                let count = bound
                    .get_mut(variable)
                    .expect("the forall binder was entered");
                *count -= 1;
                if *count == 0 {
                    bound.remove(variable);
                }
            }
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            collect_free_variables(*ty, types, bound, active, free);
            collect_free_variables(*tail, types, bound, active, free);
        }
        Some(Type::Closure { parameters, result }) => {
            for parameter in parameters {
                collect_free_variables(*parameter, types, bound, active, free);
            }
            collect_free_variables(*result, types, bound, active, free);
        }
        _ => {}
    }
    active.remove(&id);
}

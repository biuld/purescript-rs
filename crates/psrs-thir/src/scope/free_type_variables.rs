use crate::{Type, TypeId};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

pub(super) fn free_type_variables(
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
            free_type_variables(*function, types, bound, active, free);
            free_type_variables(*argument, types, bound, active, free);
        }
        Some(Type::ForAll { variables, body }) => {
            for variable in variables {
                *bound.entry(*variable).or_default() += 1;
            }
            free_type_variables(*body, types, bound, active, free);
            for variable in variables {
                if let Some(count) = bound.get_mut(variable) {
                    *count -= 1;
                    if *count == 0 {
                        bound.remove(variable);
                    }
                }
            }
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            free_type_variables(*ty, types, bound, active, free);
            free_type_variables(*tail, types, bound, active, free);
        }
        _ => {}
    }
    active.remove(&id);
}

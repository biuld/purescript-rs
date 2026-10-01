use crate::{Module, Type, TypeId, record_row};
use std::collections::{HashMap, HashSet};

pub(super) fn collect_free_variables(
    id: TypeId,
    types: &[Type],
    bound: &mut HashMap<psrs_hir::TypeVariableId, usize>,
    active: &mut HashSet<TypeId>,
    free: &mut HashSet<psrs_hir::TypeVariableId>,
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
                if let Some(count) = bound.get_mut(variable) {
                    *count -= 1;
                    if *count == 0 {
                        bound.remove(variable);
                    }
                }
            }
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            collect_free_variables(*ty, types, bound, active, free);
            collect_free_variables(*tail, types, bound, active, free);
        }
        _ => {}
    }
    active.remove(&id);
}

pub(super) fn same_type(left: TypeId, right: TypeId, module: &Module) -> bool {
    super::super::types_compatible(
        left,
        right,
        module,
        &mut HashSet::new(),
        &mut HashMap::new(),
    ) && super::super::types_compatible(
        right,
        left,
        module,
        &mut HashSet::new(),
        &mut HashMap::new(),
    )
}

pub(crate) fn equivalent_types(left: TypeId, right: TypeId, module: &Module) -> bool {
    same_type(left, right, module)
}

pub(super) fn is_record_type(module: &Module, id: TypeId) -> bool {
    record_row(&module.types, id).is_some()
}

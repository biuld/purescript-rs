use crate::{Module, TypeId, record_row};
use std::collections::{HashMap, HashSet};

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

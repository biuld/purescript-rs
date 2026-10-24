//! The source gate prevents polymorphic persistent references before erasure.
use psrs_core::{Module, Type, TypeId};
use std::collections::HashSet;

pub(super) fn validate(module: &Module, ty: TypeId, export: &str) -> Result<(), String> {
    if export != psrs_runtime::STATIC_CELL_EXPORT {
        return Err("unknown runtime-static export".into());
    }
    let mut pending = vec![ty];
    let mut seen = HashSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        match module.types.get(id.0 as usize) {
            Some(Type::Variable(_) | Type::ForAll { .. }) => {
                return Err("static cell requires a closed monomorphic signature".into());
            }
            Some(Type::Application(a, b)) => pending.extend([*a, *b]),
            Some(Type::RowExtend { ty, tail, .. }) => pending.extend([*ty, *tail]),
            Some(Type::Closure { parameters, result }) => {
                pending.extend(parameters);
                pending.push(*result);
            }
            None => return Err("static cell signature references an absent type".into()),
            _ => {}
        }
    }
    super::source::validate_contract(module, ty, psrs_runtime::STATIC_CELL_SOURCE)
}

use crate::{Module, TypeId};

pub(super) fn strip_leading_foralls(module: &Module, mut id: TypeId) -> TypeId {
    let mut seen = std::collections::HashSet::new();
    while let Some((_, body)) = crate::forall_parts(&module.types, id) {
        if !seen.insert(id) {
            break;
        }
        id = body;
    }
    id
}

/// The single parameter and result of a fixed-arity closure.
pub(super) fn closure_call(module: &Module, id: TypeId) -> Option<(TypeId, TypeId)> {
    let (parameters, result) = crate::closure_parts(&module.types, id)?;
    if parameters.len() != 1 {
        return None;
    }
    Some((parameters[0], result))
}

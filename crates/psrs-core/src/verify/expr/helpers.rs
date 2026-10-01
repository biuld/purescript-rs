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

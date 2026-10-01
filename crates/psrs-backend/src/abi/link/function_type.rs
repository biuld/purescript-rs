use psrs_core::{Module as CoreModule, TypeId as CoreTypeId};

/// Walks a Core function type into its parameter types and final result type.
pub(crate) fn function_parts(
    module: &CoreModule,
    type_id: CoreTypeId,
) -> Option<(Vec<CoreTypeId>, CoreTypeId)> {
    let mut current = type_id;
    let mut seen = std::collections::HashSet::new();
    while seen.insert(current)
        && let Some((_, body)) = psrs_core::forall_parts(&module.types, current)
    {
        current = body;
    }
    let mut parameters = Vec::new();
    loop {
        match psrs_core::arrow_parts(&module.types, current) {
            Some((parameter, result)) => {
                parameters.push(parameter);
                current = result;
                // A forall result is a polymorphic value with its own callable
                // signature, not another parameter of this imported function.
                if psrs_core::forall_parts(&module.types, current).is_some() {
                    return Some((parameters, current));
                }
            }
            None => return Some((parameters, current)),
        }
    }
}

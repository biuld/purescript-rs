//! The checked calling boundary of a Core value, before representation lowering.
use super::{TypeId, arrow_parts, closure_parts, forall_parts, scheme_parts};
use crate::Type;
use std::collections::HashSet;

/// A fixed-arity closure retains its result as a value. Ordinary arrows flatten
/// only up to a quantified or explicit-closure result boundary. Non-callable
/// values have no parameters; malformed spines are errors, never empty calls.
pub fn call_parts(types: &[Type], id: TypeId) -> Result<(Vec<TypeId>, TypeId), &'static str> {
    let (_, id) = scheme_parts(types, id).ok_or("callable has a dangling or cyclic scheme")?;
    if let Some((parameters, result)) = closure_parts(types, id) {
        if parameters
            .iter()
            .chain(std::iter::once(&result))
            .any(|ty| ty.0 as usize >= types.len())
        {
            return Err("closure signature has a dangling type");
        }
        return Ok((parameters.to_vec(), result));
    }
    let mut parameters = Vec::new();
    let mut current = id;
    let mut visited = HashSet::new();
    while let Some((parameter, result)) = arrow_parts(types, current) {
        if !visited.insert(current) {
            return Err("callable has a cyclic arrow spine");
        }
        if parameter.0 as usize >= types.len() || result.0 as usize >= types.len() {
            return Err("function signature has a dangling type");
        }
        parameters.push(parameter);
        current = result;
        if forall_parts(types, current).is_some() {
            break;
        }
    }
    Ok((parameters, current))
}

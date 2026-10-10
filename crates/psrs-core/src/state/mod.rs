//! Shared source contracts for state-transforming callables.
//!
//! These contracts refer only to registered primitive identities and checked
//! structural records. They do not recognize a library's action constructor.
use crate::{Module, Type, TypeConstructor, TypeId, call_parts, row_fields};
use psrs_hir::TypeId as HirTypeId;

pub mod dependency;
pub mod flow;
pub mod primitive;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateSignature {
    /// Ordinary arguments evaluated before the final state invocation.
    pub parameters: Vec<TypeId>,
    pub state: TypeId,
    pub region: TypeId,
    pub payload: TypeId,
}

/// Reads State's exactly one nominal region argument. A dangling or unsaturated
/// application is not a valid state value.
pub fn region(module: &Module, ty: TypeId) -> Option<TypeId> {
    let (head, arguments) = module.applied_constructor(ty)?;
    if head != TypeConstructor::User(HirTypeId::PRIM_STATE) || arguments.len() != 1 {
        return None;
    }
    Some(arguments[0])
}

/// Reads the complete structural Step contract, rejecting open rows, duplicate
/// fields, extra fields and state/region mismatch.
pub fn step(module: &Module, ty: TypeId) -> Option<(TypeId, TypeId)> {
    let Type::Application(head, row) = module.types.get(ty.0 as usize)? else {
        return None;
    };
    if module.types.get(head.0 as usize) != Some(&Type::Constructor(TypeConstructor::Record)) {
        return None;
    }
    let (fields, tail) = row_fields(&module.types, *row)?;
    if tail.is_some() || fields.len() != 2 {
        return None;
    }
    let state = fields.iter().find(|(name, _)| name == "state")?.1;
    let payload = fields.iter().find(|(name, _)| name == "value")?.1;
    region(module, state)?;
    Some((state, payload))
}

/// Validates an ordinary checked function's state input and result. No source
/// constructor or module name is interpreted as evidence.
pub fn signature(module: &Module, ty: TypeId) -> Result<StateSignature, &'static str> {
    let (parameters, result) = call_parts(&module.types, ty)?;
    for (index, parameter) in parameters.iter().copied().enumerate() {
        if let Some(region) = region(module, parameter) {
            if index + 1 != parameters.len() {
                return Err("state invocation parameter must be last in its call boundary");
            }
            let (result_state, payload) = step(module, result)
                .ok_or("state function must return a closed state/value record")?;
            if !module.types_equivalent(parameter, result_state) {
                return Err("state function returns a different region");
            }
            return Ok(StateSignature {
                parameters: parameters[..index].to_vec(),
                state: parameter,
                region,
                payload,
            });
        }
    }
    Err("state function has no state invocation parameter")
}

#[cfg(test)]
mod tests;

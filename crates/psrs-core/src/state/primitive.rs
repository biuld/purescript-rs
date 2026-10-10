//! Type contracts for checked state primitives before representation erasure.
use super::signature;
use crate::{Module, Type, TypeConstructor, TypeId, scheme_parts};
use psrs_hir::{Intrinsic, StateOperation, TypeId as HirTypeId};
use std::collections::HashSet;

pub fn verify(
    module: &Module,
    intrinsic: Intrinsic,
    arguments: &[TypeId],
    result: TypeId,
) -> Result<(), &'static str> {
    let operation = intrinsic
        .state_operation()
        .ok_or("operation has no state contract")?;
    if arguments.len() != intrinsic.descriptor().arity as usize {
        return Err("state primitive has an invalid argument count");
    }
    let checked = signature(module, arguments[0])?;
    if !checked.parameters.is_empty() || !module.types_equivalent(checked.payload, result) {
        return Err("state runner has an invalid callable payload");
    }
    match operation {
        StateOperation::RunWorld => {
            if module.types.get(checked.region.0 as usize)
                != Some(&Type::Constructor(TypeConstructor::User(
                    HirTypeId::PRIM_REAL_WORLD,
                )))
            {
                return Err("world runner requires the RealWorld region");
            }
        }
        StateOperation::RunRegion => {
            let (variables, _) = scheme_parts(&module.types, arguments[0])
                .ok_or("region runner has an invalid quantified callable")?;
            let Some(Type::Variable(region)) = module.types.get(checked.region.0 as usize) else {
                return Err("region runner requires a quantified region");
            };
            if variables.as_slice() != [*region] {
                return Err("region runner requires exactly one fresh region binder");
            }
            if mentions(module, checked.payload, checked.region, &mut HashSet::new()) {
                return Err("region runner result escapes its region");
            }
        }
    }
    Ok(())
}

fn mentions(module: &Module, ty: TypeId, region: TypeId, seen: &mut HashSet<TypeId>) -> bool {
    if !seen.insert(ty) {
        return false;
    }
    if module.types_equivalent(ty, region) {
        return true;
    }
    match module.types.get(ty.0 as usize) {
        Some(Type::Application(head, argument)) => {
            mentions(module, *head, region, seen) || mentions(module, *argument, region, seen)
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            mentions(module, *ty, region, seen) || mentions(module, *tail, region, seen)
        }
        Some(Type::ForAll { body, .. }) => mentions(module, *body, region, seen),
        Some(Type::Closure { parameters, result }) => {
            parameters
                .iter()
                .any(|ty| mentions(module, *ty, region, seen))
                || mentions(module, *result, region, seen)
        }
        _ => false,
    }
}

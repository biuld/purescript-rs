//! Callable protocols derived from checked transparent newtype fields.
use super::ProtocolParameter;
use psrs_core::{Module, Type, TypeConstructor, TypeId};
use psrs_hir::{TypeId as HirTypeId, TypeVariableId};
use std::collections::{HashMap, HashSet};

pub(super) fn parameters(module: &Module, id: HirTypeId) -> Option<Vec<ProtocolParameter>> {
    let constructor = module
        .constructors
        .iter()
        .find(|constructor| constructor.type_id == id)?;
    if constructor.field_types.len() != 1 {
        return None;
    }
    let bindings = constructor
        .parameters
        .iter()
        .enumerate()
        .map(|(index, variable)| (*variable, ProtocolParameter::Argument(index)))
        .collect();
    let mut visiting = HashSet::from([id]);
    callable(module, constructor.field_types[0], &bindings, &mut visiting)
}

fn callable(
    module: &Module,
    mut ty: TypeId,
    bindings: &HashMap<TypeVariableId, ProtocolParameter>,
    visiting: &mut HashSet<HirTypeId>,
) -> Option<Vec<ProtocolParameter>> {
    while let Some((_, body)) = psrs_core::forall_parts(&module.types, ty) {
        ty = body;
    }
    if let Some((parameters, _)) = psrs_core::closure_parts(&module.types, ty) {
        return parameters
            .iter()
            .map(|ty| domain(module, *ty, bindings))
            .collect();
    }
    let mut parameters = Vec::new();
    let mut cursor = ty;
    while let Some((parameter, result)) = psrs_core::arrow_parts(&module.types, cursor) {
        parameters.push(domain(module, parameter, bindings)?);
        cursor = result;
        if psrs_core::forall_parts(&module.types, cursor).is_some() {
            break;
        }
    }
    if !parameters.is_empty() {
        return Some(parameters);
    }
    let (TypeConstructor::User(id), arguments) = module.applied_constructor(ty)? else {
        return None;
    };
    if !module.newtype_ids.contains(&id) || !visiting.insert(id) {
        return None;
    }
    let constructor = module
        .constructors
        .iter()
        .find(|constructor| constructor.type_id == id)?;
    if constructor.field_types.len() != 1 || constructor.parameters.len() != arguments.len() {
        return None;
    }
    let nested = constructor
        .parameters
        .iter()
        .zip(arguments)
        .map(|(variable, argument)| Some((*variable, domain(module, argument, bindings)?)))
        .collect::<Option<HashMap<_, _>>>()?;
    let result = callable(module, constructor.field_types[0], &nested, visiting);
    visiting.remove(&id);
    result
}

fn domain(
    module: &Module,
    ty: TypeId,
    bindings: &HashMap<TypeVariableId, ProtocolParameter>,
) -> Option<ProtocolParameter> {
    if let Some(Type::Variable(variable)) = module.types.get(ty.0 as usize) {
        return bindings.get(variable).cloned();
    }
    closed(module, ty, &HashSet::new(), &mut HashSet::new()).then_some(ProtocolParameter::Fixed(ty))
}

fn closed(
    module: &Module,
    ty: TypeId,
    bound: &HashSet<TypeVariableId>,
    active: &mut HashSet<TypeId>,
) -> bool {
    if !active.insert(ty) {
        return false;
    }
    let result = match module.types.get(ty.0 as usize) {
        Some(Type::Variable(variable)) => bound.contains(variable),
        Some(Type::Application(function, argument)) => {
            closed(module, *function, bound, active) && closed(module, *argument, bound, active)
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            closed(module, *ty, bound, active) && closed(module, *tail, bound, active)
        }
        Some(Type::ForAll { variables, body }) => {
            let mut bound = bound.clone();
            bound.extend(variables);
            closed(module, *body, &bound, active)
        }
        Some(Type::Closure { parameters, result }) => {
            parameters
                .iter()
                .all(|ty| closed(module, *ty, bound, active))
                && closed(module, *result, bound, active)
        }
        Some(_) => true,
        None => false,
    };
    active.remove(&ty);
    result
}

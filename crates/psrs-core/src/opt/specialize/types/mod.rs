mod substitution;

use crate::{Declaration, Module, Type, TypeConstructor, TypeId};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum TypeKey {
    Constructor(TypeConstructor),
    Application(Box<TypeKey>, Box<TypeKey>),
    Record(Vec<(String, TypeKey)>),
}

pub(super) fn concrete_type_key(module: &Module, id: TypeId) -> Option<TypeKey> {
    type_key(module, id, &mut HashSet::new())
}

pub(super) fn match_instantiation(
    module: &Module,
    declaration: &Declaration,
    call_type: TypeId,
) -> Option<(HashMap<TypeVariableId, TypeId>, Vec<TypeKey>)> {
    if declaration.quantified.is_empty() || concrete_type_key(module, call_type).is_none() {
        return None;
    }
    let quantifiers = declaration
        .quantified
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut replacements = HashMap::new();
    if !match_type(
        module,
        declaration.ty,
        call_type,
        &quantifiers,
        &mut replacements,
        &mut HashSet::new(),
    ) || replacements.len() != quantifiers.len()
    {
        return None;
    }
    let key = declaration
        .quantified
        .iter()
        .map(|variable| concrete_type_key(module, *replacements.get(variable)?))
        .collect::<Option<Vec<_>>>()?;
    Some((replacements, key))
}

pub(super) use substitution::instantiate_declaration;

fn type_key(module: &Module, id: TypeId, active: &mut HashSet<TypeId>) -> Option<TypeKey> {
    if !active.insert(id) {
        return None;
    }
    let result = if let Some(row) = module.record_row(id) {
        record_key(module, row, active)
    } else {
        match module.types.get(id.0 as usize)? {
            Type::Variable(_) => None,
            Type::Constructor(constructor) => Some(TypeKey::Constructor(*constructor)),
            Type::Application(function, argument) => Some(TypeKey::Application(
                Box::new(type_key(module, *function, active)?),
                Box::new(type_key(module, *argument, active)?),
            )),
            Type::ForAll { .. }
            | Type::RowEmpty
            | Type::RowExtend { .. }
            | Type::Closure { .. } => None,
        }
    };
    active.remove(&id);
    result
}

/// A concrete key for a record row: closed rows only, with the same field
/// labels and recursively concrete field types.
fn record_key(module: &Module, row: TypeId, active: &mut HashSet<TypeId>) -> Option<TypeKey> {
    let (fields, tail) = module.row_fields(row)?;
    if tail.is_some() {
        return None;
    }
    let mut keys = fields
        .iter()
        .map(|(label, field)| Some((label.clone(), type_key(module, *field, active)?)))
        .collect::<Option<Vec<_>>>()?;
    keys.sort_by(|left, right| left.0.cmp(&right.0));
    Some(TypeKey::Record(keys))
}

fn is_record_head(module: &Module, id: TypeId) -> bool {
    matches!(
        module.types.get(id.0 as usize),
        Some(Type::Constructor(TypeConstructor::Record))
    )
}

fn match_type(
    module: &Module,
    generic: TypeId,
    concrete: TypeId,
    quantifiers: &HashSet<TypeVariableId>,
    replacements: &mut HashMap<TypeVariableId, TypeId>,
    active: &mut HashSet<(TypeId, TypeId)>,
) -> bool {
    if !active.insert((generic, concrete)) {
        return true;
    }
    let (Some(generic_type), Some(concrete_type)) = (
        module.types.get(generic.0 as usize),
        module.types.get(concrete.0 as usize),
    ) else {
        return false;
    };
    if let Type::Variable(variable) = generic_type
        && quantifiers.contains(variable)
    {
        let Some(key) = concrete_type_key(module, concrete) else {
            return false;
        };
        if let Some(previous) = replacements.get(variable) {
            return concrete_type_key(module, *previous).as_ref() == Some(&key);
        }
        replacements.insert(*variable, concrete);
        return true;
    }
    match (generic_type, concrete_type) {
        (Type::Constructor(left), Type::Constructor(right)) => left == right,
        (Type::Application(gf, ga), Type::Application(cf, ca)) => {
            if is_record_head(module, *gf) && is_record_head(module, *cf) {
                match_record_rows(module, *ga, *ca, quantifiers, replacements, active)
            } else {
                match_type(module, *gf, *cf, quantifiers, replacements, active)
                    && match_type(module, *ga, *ca, quantifiers, replacements, active)
            }
        }
        _ => false,
    }
}

fn match_record_rows(
    module: &Module,
    generic_row: TypeId,
    concrete_row: TypeId,
    quantifiers: &HashSet<TypeVariableId>,
    replacements: &mut HashMap<TypeVariableId, TypeId>,
    active: &mut HashSet<(TypeId, TypeId)>,
) -> bool {
    let (Some((generic_fields, _)), Some((concrete_fields, _))) = (
        module.row_fields(generic_row),
        module.row_fields(concrete_row),
    ) else {
        return false;
    };
    if generic_fields.len() != concrete_fields.len() {
        return false;
    }
    generic_fields.iter().all(|(label, generic_field)| {
        concrete_fields
            .iter()
            .find(|(other, _)| other == label)
            .is_some_and(|(_, concrete_field)| {
                match_type(
                    module,
                    *generic_field,
                    *concrete_field,
                    quantifiers,
                    replacements,
                    active,
                )
            })
    })
}

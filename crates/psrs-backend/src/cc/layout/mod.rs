use super::VariantCase;
use super::{ReprId, Representation, RepresentationTable, Signature, SignatureId, ValueShape};
use crate::BackendError;
use psrs_core::{Module as CoreModule, Type, TypeConstructor, TypeId};
use psrs_hir::{SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod captures;
mod functions;
mod scalar;

#[cfg(test)]
mod tests;

use captures::module_has_integer_capture;
pub(crate) use functions::function_signature;
use scalar::field_storage_shape;
pub(super) use scalar::{declaration_shape, scalar_type};

pub(super) fn enum_type_ids(
    module: &CoreModule,
    newtype_ids: &HashSet<HirTypeId>,
) -> HashSet<HirTypeId> {
    let mut all_nullary: HashMap<HirTypeId, bool> = HashMap::new();
    for constructor in &module.constructors {
        let entry = all_nullary.entry(constructor.type_id).or_insert(true);
        if constructor.field_count != 0 {
            *entry = false;
        }
    }
    all_nullary
        .into_iter()
        .filter(|(id, nullary)| *nullary && !newtype_ids.contains(id))
        .map(|(id, _)| id)
        .collect()
}

pub(super) fn aggregate_type_ids(
    module: &CoreModule,
    newtype_ids: &HashSet<HirTypeId>,
) -> HashSet<HirTypeId> {
    module
        .constructors
        .iter()
        .filter(|constructor| {
            !newtype_ids.contains(&constructor.type_id)
                && constructor.field_count != 0
                && constructor
                    .field_types
                    .iter()
                    .all(|field| layoutable_field_type(module, *field, newtype_ids))
        })
        .map(|constructor| constructor.type_id)
        .collect()
}

fn layoutable_field_type(
    module: &CoreModule,
    id: TypeId,
    newtype_ids: &HashSet<HirTypeId>,
) -> bool {
    let mut visiting = HashSet::new();
    layoutable_field_type_inner(module, id, newtype_ids, &mut visiting)
}

fn layoutable_field_type_inner(
    module: &CoreModule,
    id: TypeId,
    newtype_ids: &HashSet<HirTypeId>,
    visiting: &mut HashSet<HirTypeId>,
) -> bool {
    if depends_on_type_variable(module, id) {
        return true;
    }
    match module.types.get(id.0 as usize) {
        Some(Type::I32 | Type::Boolean | Type::F64 | Type::Char | Type::String | Type::Unit) => {
            true
        }
        Some(Type::Constructor(TypeConstructor::User(type_id)))
            if newtype_ids.contains(type_id) =>
        {
            let Some(inner) = newtype_field_type(module, *type_id) else {
                return false;
            };
            if !visiting.insert(*type_id) {
                return false;
            }
            let result = layoutable_field_type_inner(module, inner, newtype_ids, visiting);
            visiting.remove(type_id);
            result
        }
        Some(Type::Constructor(TypeConstructor::User(_))) => true,
        Some(Type::Variable(_))
        | Some(Type::Constructor(TypeConstructor::Array))
        | Some(Type::Record(_))
        | Some(Type::Function { .. })
        | None => false,
        Some(Type::Application(_, _)) => array_element_type(module, id).is_some(),
    }
}

/// Builds target-neutral representation requirements. No concrete Wasm type or
/// index is allocated here; P9 owns that conversion.
pub(super) fn type_layout(
    module: &CoreModule,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
) -> Result<TypeLayout, Vec<BackendError>> {
    let mut representations = RepresentationTable::default();

    let boxed_integer_type = if module
        .types
        .iter()
        .any(|ty| matches!(ty, Type::Variable(_)))
        || module
            .declarations
            .iter()
            .any(|declaration| depends_on_type_variable(module, declaration.ty))
        || module_has_integer_capture(module)
        || module
            .constructors
            .iter()
            .flat_map(|constructor| &constructor.field_types)
            .any(|field| depends_on_type_variable(module, *field))
    {
        let id = representations.reserve();
        representations.set(
            id,
            Representation::Box {
                value: ValueShape::Integer,
            },
        );
        Some(id)
    } else {
        None
    };
    let boxed_number_type = if module.types.iter().any(|ty| matches!(ty, Type::F64)) {
        let id = representations.reserve();
        representations.set(
            id,
            Representation::Box {
                value: ValueShape::Number,
            },
        );
        Some(id)
    } else {
        None
    };

    let array_ids = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, _)| {
            let id = TypeId(index as u32);
            array_element_type(module, id).map(|_| id)
        })
        .collect::<Vec<_>>();
    let mut array_types = HashMap::new();
    for id in &array_ids {
        array_types.insert(*id, representations.reserve());
    }

    let record_ids = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, ty)| matches!(ty, Type::Record(_)).then_some(TypeId(index as u32)))
        .collect::<Vec<_>>();
    let mut record_types = HashMap::new();
    for id in &record_ids {
        record_types.insert(*id, representations.reserve());
    }

    // Function signatures can refer to record and array references, so reserve
    // those logical layouts first. Product fields can then use the completed
    // closure signature table, including dictionary methods stored as fields.
    let function_layout = functions::append_function_types(
        module,
        enum_types,
        aggregate_types,
        newtype_ids,
        &array_types,
        &record_types,
        &mut representations,
    )?;

    for id in &array_ids {
        let Some(element) = array_element_type(module, *id) else {
            continue;
        };
        let element = scalar_type(
            module,
            element,
            module.span,
            enum_types,
            aggregate_types,
            newtype_ids,
            &array_types,
            &record_types,
            &function_layout.function_types,
        )?;
        representations.set(array_types[id], Representation::Array { element });
    }
    for id in &record_ids {
        let Type::Record(fields) = &module.types[id.0 as usize] else {
            continue;
        };
        let fields = fields
            .iter()
            .map(|(_, field)| {
                field_storage_shape(
                    module,
                    *field,
                    module.span,
                    enum_types,
                    aggregate_types,
                    newtype_ids,
                    &array_types,
                    &record_types,
                    &function_layout.function_types,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        representations.set(record_types[id], Representation::Product { fields });
    }

    let mut constructor_types = HashMap::new();
    let mut aggregate_ids = aggregate_types.iter().copied().collect::<Vec<_>>();
    aggregate_ids.sort_by_key(|id| (id.module.0, id.index));
    for type_id in aggregate_ids {
        let id = representations.reserve();
        let mut cases = Vec::new();
        for constructor in module
            .constructors
            .iter()
            .filter(|constructor| constructor.type_id == type_id)
        {
            if constructor.field_types.len() != constructor.field_count {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    module.span,
                    "constructor field metadata is inconsistent",
                )]);
            }
            constructor_types.insert(constructor.symbol, id);
            let fields = constructor
                .field_types
                .iter()
                .map(|field| {
                    field_storage_shape(
                        module,
                        *field,
                        module.span,
                        enum_types,
                        aggregate_types,
                        newtype_ids,
                        &array_types,
                        &record_types,
                        &function_layout.function_types,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            cases.push(VariantCase {
                tag: constructor.tag,
                fields,
            });
        }
        representations.set(id, Representation::Variant { cases });
    }

    Ok(TypeLayout {
        representations,
        array_types,
        record_types,
        constructor_types,
        boxed_integer_type,
        boxed_number_type,
        function_types: function_layout.function_types,
    })
}

pub(super) struct TypeLayout {
    pub(super) representations: RepresentationTable,
    pub(super) array_types: HashMap<TypeId, ReprId>,
    pub(super) record_types: HashMap<TypeId, ReprId>,
    pub(super) constructor_types: HashMap<SymbolId, ReprId>,
    pub(super) boxed_integer_type: Option<ReprId>,
    pub(super) boxed_number_type: Option<ReprId>,
    pub(super) function_types: HashMap<TypeId, SignatureId>,
}

pub(super) fn newtype_field_type(module: &CoreModule, type_id: HirTypeId) -> Option<TypeId> {
    let mut constructors = module
        .constructors
        .iter()
        .filter(|constructor| constructor.type_id == type_id);
    let constructor = constructors.next()?;
    if constructors.next().is_some() || constructor.field_count != 1 {
        return None;
    }
    constructor.field_types.first().copied()
}

pub(super) fn user_type_id(module: &CoreModule, mut id: TypeId) -> Option<HirTypeId> {
    loop {
        match module.types.get(id.0 as usize)? {
            Type::Constructor(TypeConstructor::User(type_id)) => return Some(*type_id),
            Type::Application(function, _) => id = *function,
            _ => return None,
        }
    }
}

pub(super) fn layout_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}

fn array_element_type(module: &CoreModule, id: TypeId) -> Option<TypeId> {
    let Type::Application(function, element) = module.types.get(id.0 as usize)? else {
        return None;
    };
    match module.types.get(function.0 as usize)? {
        Type::Constructor(TypeConstructor::Array) => Some(*element),
        _ => None,
    }
}

pub(super) fn depends_on_type_variable(module: &CoreModule, id: TypeId) -> bool {
    fn visit(module: &CoreModule, id: TypeId, visiting: &mut HashSet<TypeId>) -> bool {
        if !visiting.insert(id) {
            return false;
        }
        let result = match module.types.get(id.0 as usize) {
            Some(Type::Variable(_)) => true,
            Some(Type::Application(function, argument))
            | Some(Type::Function {
                parameter: function,
                result: argument,
            }) => visit(module, *function, visiting) || visit(module, *argument, visiting),
            Some(Type::Record(fields)) => fields
                .iter()
                .any(|(_, field)| visit(module, *field, visiting)),
            _ => false,
        };
        visiting.remove(&id);
        result
    }

    visit(module, id, &mut HashSet::new())
}

pub(super) fn erased_field_recovery_family(
    module: &CoreModule,
    id: TypeId,
    stored: ValueShape,
    expected: ValueShape,
    array_types: &HashMap<TypeId, ReprId>,
    record_types: &HashMap<TypeId, ReprId>,
) -> Option<&'static str> {
    if !matches!(
        stored,
        ValueShape::Reference(crate::cc::Reference {
            heap: crate::cc::RefShape::Erased,
            ..
        })
    ) || !depends_on_type_variable(module, id)
    {
        return None;
    }
    let ValueShape::Reference(reference) = expected else {
        return None;
    };
    let crate::cc::RefShape::Repr(representation) = reference.heap else {
        return None;
    };
    if array_types.get(&id) == Some(&representation) {
        Some("array")
    } else if record_types.get(&id) == Some(&representation) {
        Some("record")
    } else {
        None
    }
}

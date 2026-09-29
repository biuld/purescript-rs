use super::VariantCase;
use super::{ReprId, Representation, RepresentationTable, Signature, SignatureId, ValueShape};
use crate::BackendError;
use psrs_core::{Module as CoreModule, Type, TypeConstructor, TypeId};
use psrs_hir::{SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod aggregate;
mod captures;
mod functions;
mod scalar;

#[cfg(test)]
mod tests;

use captures::module_has_integer_capture;
pub(crate) use functions::function_signature;
use scalar::field_storage_shape;
pub(super) use scalar::{declaration_shape, scalar_type};

/// The runtime value shape of a Core primitive constructor, keyed by the
/// constructor. This is the single mapping from a source primitive to its
/// runtime representation; every layout site consults it rather than matching
/// the scalar node itself.
pub(super) fn primitive_value_shape(constructor: TypeConstructor) -> Option<ValueShape> {
    Some(match constructor {
        TypeConstructor::Int | TypeConstructor::Char | TypeConstructor::Unit => ValueShape::Integer,
        TypeConstructor::String => ValueShape::String,
        TypeConstructor::Number => ValueShape::Number,
        TypeConstructor::Boolean => ValueShape::Boolean,
        TypeConstructor::Function
        | TypeConstructor::Record
        | TypeConstructor::Array
        | TypeConstructor::User(_) => {
            return None;
        }
    })
}

/// The primitive shape of a Core type when its head is a primitive constructor.
pub(super) fn primitive_shape_of(module: &CoreModule, id: TypeId) -> Option<ValueShape> {
    match module.types.get(id.0 as usize)? {
        Type::Constructor(constructor) => primitive_value_shape(*constructor),
        _ => None,
    }
}

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
    if primitive_shape_of(module, id).is_some() {
        return true;
    }
    match module.types.get(id.0 as usize) {
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
        // A closed record has its own representation handle, so a variant case
        // may carry one as a referenced payload (WIT `datetime`, socket
        // addresses, and directory entries are records). An open row has no
        // fixed layout.
        Some(_) if module.is_record_type(id) => !module.record_is_open(id).unwrap_or(false),
        Some(Type::Variable(_))
        | Some(Type::Constructor(_))
        | Some(Type::RowEmpty)
        | Some(Type::RowExtend { .. })
        | None => false,
        Some(Type::Application(_, _)) => {
            if array_element_type(module, id).is_some() {
                return true;
            }
            // An applied newtype such as `Resource a` erases to its single
            // field, so a variant case may carry it as a storage field.
            let Some(type_id) = user_type_id(module, id) else {
                return false;
            };
            if !newtype_ids.contains(&type_id) {
                return false;
            }
            let Some(inner) = newtype_field_type(module, type_id) else {
                return false;
            };
            if !visiting.insert(type_id) {
                return false;
            }
            let result = layoutable_field_type_inner(module, inner, newtype_ids, visiting);
            visiting.remove(&type_id);
            result
        }
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
    let boxed_number_type = if module.types.iter().any(|ty| {
        matches!(ty, Type::Constructor(c) if primitive_value_shape(*c) == Some(ValueShape::Number))
    }) {
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

    // Reserve aggregate handles before signatures so closure types can refer
    // to them. Normalize afterward and rewrite signature references to the
    // canonical handles.
    let reserved_aggregates = aggregate::reserve_aggregate_layouts(module, &mut representations);
    let function_layout = functions::append_function_types(
        module,
        enum_types,
        aggregate_types,
        newtype_ids,
        &reserved_aggregates.arrays,
        &reserved_aggregates.records,
        &mut representations,
    )?;
    let (aggregate_layouts, function_types) = aggregate::normalize_aggregate_layouts(
        module,
        enum_types,
        aggregate_types,
        newtype_ids,
        function_layout.function_types,
        reserved_aggregates,
        &mut representations,
    )?;
    let array_types = aggregate_layouts.arrays;
    let record_types = aggregate_layouts.records;

    let mut constructor_types = HashMap::new();
    let mut aggregate_ids = aggregate_types.iter().copied().collect::<Vec<_>>();
    aggregate_ids.sort_by_key(|id| (id.module.0, id.index));
    for type_id in aggregate_ids {
        let id = representations.reserve();
        let mut cases = Vec::new();
        let mut constructors = module
            .constructors
            .iter()
            .filter(|constructor| constructor.type_id == type_id)
            .collect::<Vec<_>>();
        constructors.sort_by_key(|constructor| constructor.tag);
        for constructor in constructors {
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
                        &function_types,
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
        function_types,
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

/// A callable type-constructor application: a head constructor registered by
/// the trusted elaboration whose application is a closure. Returns its hidden
/// calling-convention parameter count and the value type the call produces (the
/// application's last type argument).
pub(super) fn callable_application(module: &CoreModule, id: TypeId) -> Option<(u32, TypeId)> {
    let (type_id, arguments) = module.callable_application(id)?;
    let parameters = module.callable_parameters(type_id)?;
    Some((parameters, *arguments.last()?))
}

/// Whether a Core type is a callable closure value: an ordinary function arrow
/// or a registered callable type-constructor application.
pub(super) fn is_callable_type(module: &CoreModule, id: TypeId) -> bool {
    psrs_core::arrow_parts(&module.types, id).is_some()
        || callable_application(module, id).is_some()
}

/// Derives the flattened calling convention of a value from the application
/// spine and the head constructor's representation.
///
/// An ordinary function flattens every arrow: the parameters are the arrow
/// domains and the result is the codomain. A registered callable constructor
/// application contributes its hidden calling-convention parameters and the
/// call result is the application's last type argument, so an effect's returned
/// function is a separate closure rather than extra parameters of the effect.
pub(crate) fn function_arrow_parameters(module: &CoreModule, id: TypeId) -> (Vec<TypeId>, TypeId) {
    if let Some((parameters, result)) = callable_application(module, id) {
        let context = context_parameter_type(module);
        return (vec![context; parameters as usize], result);
    }
    let mut parameters = Vec::new();
    let mut current = id;
    while let Some((parameter, result)) = psrs_core::arrow_parts(&module.types, current) {
        parameters.push(parameter);
        current = result;
    }
    (parameters, current)
}

/// A representative integer-shaped Core type for a closure's hidden context
/// parameter. The context is a calling-convention scalar, not a token type; any
/// integer-shaped Core type stands for its shape. Falls back to an out-of-range
/// id only when the module has no integer scalar to name.
pub(super) fn context_parameter_type(module: &CoreModule) -> TypeId {
    if let Some(index) = module.types.iter().position(|ty| {
        matches!(ty, Type::Constructor(c)
        if matches!(
            primitive_value_shape(*c),
            Some(ValueShape::Integer | ValueShape::Boolean)
        ))
    }) {
        return TypeId(index as u32);
    }
    TypeId(u32::MAX)
}

pub(super) fn layout_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}

pub(super) fn array_element_type(module: &CoreModule, id: TypeId) -> Option<TypeId> {
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
            Some(Type::Application(function, argument)) => {
                visit(module, *function, visiting) || visit(module, *argument, visiting)
            }
            Some(Type::RowExtend { ty, tail, .. }) => {
                visit(module, *ty, visiting) || visit(module, *tail, visiting)
            }
            Some(Type::RowEmpty) => false,
            _ => false,
        };
        visiting.remove(&id);
        result
    }

    visit(module, id, &mut HashSet::new())
}

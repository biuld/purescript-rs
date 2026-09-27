//! Source ABI shapes for WIT imports. CC sees [`ValueShape`], not WIT names.
//!
//! The declaration's resolved source type is interned in the Core type table
//! and carried as a `type_id`. CC derives the abstract signature directly from
//! that Core type, so a foreign signature shares the canonical layout of the
//! same type used elsewhere in the module. The recursive guest layout is read
//! separately from the representation table through
//! [`super::representation::guest_layout`].

use super::{GuestLayout, RefShape, Reference, ReprId, Signature, ValueShape, VariantCase};
use psrs_core::{Module as CoreModule, Type as CoreType, TypeConstructor, TypeId};
use psrs_hir::{SymbolId, TypeId as HirTypeId};
use std::collections::HashMap;

pub(crate) fn abstract_signature(
    type_id: Option<TypeId>,
    module: &CoreModule,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    constructor_types: &HashMap<SymbolId, ReprId>,
) -> Option<Signature> {
    let type_reprs = type_representations(module, constructor_types);
    let (parameter_ids, result_id) = crate::abi::link::function_parts(module, type_id?)?;
    let parameters = parameter_ids
        .into_iter()
        .map(|parameter| payload_shape(module, parameter, record_types, array_types, &type_reprs))
        .collect::<Option<Vec<_>>>()?;
    let result = payload_shape(module, result_id, record_types, array_types, &type_reprs)?;
    Some(Signature { parameters, result })
}

/// The concrete decode layout of a foreign import's result.
///
/// A parameterized source ADT stores type-dependent fields as `Erased`
/// (DEC-07), so its representation's case fields cannot describe what the WIT
/// result's concrete payload decodes to. This tree walks the resolved result
/// type and records, for each case, the concrete source shape of its payload.
/// MIR decodes that shape and then erases the reference into the erased storage
/// field. A non-aggregate result yields `None`.
pub(crate) fn abstract_result_guest(
    type_id: Option<TypeId>,
    module: &CoreModule,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    constructor_types: &HashMap<SymbolId, ReprId>,
) -> Option<GuestLayout> {
    let type_reprs = type_representations(module, constructor_types);
    let (_, result_id) = crate::abi::link::function_parts(module, type_id?)?;
    decode_layout(
        module,
        result_id,
        record_types,
        array_types,
        &type_reprs,
        constructor_types,
    )
}

/// The recursive concrete decode layout of a source type. A scalar or string is
/// a scalar layout; a record or array keeps its specialisation; a user data type
/// is a variant whose case fields are the concrete payload shapes of the type at
/// its resolved arguments.
fn decode_layout(
    module: &CoreModule,
    id: TypeId,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    type_reprs: &HashMap<HirTypeId, ReprId>,
    constructor_types: &HashMap<SymbolId, ReprId>,
) -> Option<GuestLayout> {
    if super::layout::depends_on_type_variable(module, id) {
        return Some(GuestLayout::Scalar {
            shape: erased_shape(),
        });
    }
    Some(match module.types.get(id.0 as usize)? {
        CoreType::I32 | CoreType::Char | CoreType::Unit => GuestLayout::Scalar {
            shape: ValueShape::Integer,
        },
        CoreType::Boolean => GuestLayout::Scalar {
            shape: ValueShape::Boolean,
        },
        CoreType::F64 => GuestLayout::Scalar {
            shape: ValueShape::Number,
        },
        CoreType::String => GuestLayout::Scalar {
            shape: ValueShape::String,
        },
        CoreType::Record(fields) => {
            let repr = *record_types.get(&id)?;
            let mut labelled = fields
                .iter()
                .map(|(label, field)| {
                    payload_shape(module, *field, record_types, array_types, type_reprs)
                        .map(|shape| (label.clone(), shape))
                })
                .collect::<Option<Vec<_>>>()?;
            labelled.sort_by(|left, right| left.0.cmp(&right.0));
            GuestLayout::Product {
                repr,
                labels: labelled.iter().map(|(label, _)| label.clone()).collect(),
                fields: labelled.into_iter().map(|(_, shape)| shape).collect(),
            }
        }
        CoreType::Application(function, element) if is_array(module, *function) => {
            let repr = *array_types.get(&id)?;
            GuestLayout::Array {
                repr,
                element: payload_shape(module, *element, record_types, array_types, type_reprs)?,
            }
        }
        CoreType::Constructor(TypeConstructor::User(_)) | CoreType::Application(_, _) => {
            decode_variant(
                module,
                id,
                record_types,
                array_types,
                type_reprs,
                constructor_types,
            )?
        }
        _ => return None,
    })
}

/// The concrete variant layout of a user data type, keyed at its resolved
/// arguments. `Maybe` and `Either` map their case payloads to the application's
/// type arguments; any other (non-parameterized) data type uses the concrete
/// declared field types.
fn decode_variant(
    module: &CoreModule,
    id: TypeId,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    type_reprs: &HashMap<HirTypeId, ReprId>,
    constructor_types: &HashMap<SymbolId, ReprId>,
) -> Option<GuestLayout> {
    let head = head_user_type(module, id)?;
    let arguments = application_arguments(module, id);
    let constructors = constructors_of(module, head);
    if constructors.is_empty() || constructors.iter().all(|case| case.field_count == 0) {
        return Some(GuestLayout::Scalar {
            shape: ValueShape::Integer,
        });
    }
    let repr = *constructor_types.get(&constructors[0].symbol)?;
    let name = user_type_name(module, head);
    let cases = constructors
        .iter()
        .map(|constructor| {
            let fields = case_field_shapes(
                module,
                name,
                &arguments,
                constructor,
                record_types,
                array_types,
                type_reprs,
            )?;
            Some(VariantCase {
                tag: constructor.tag,
                fields,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(GuestLayout::Variant { repr, cases })
}

/// The concrete source shapes of one constructor's fields at a resolved
/// application. A `Just`/`Left`/`Right` field is the matching type argument.
#[allow(clippy::too_many_arguments)]
fn case_field_shapes(
    module: &CoreModule,
    name: Option<&str>,
    arguments: &[TypeId],
    constructor: &psrs_core::ConstructorInfo,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    type_reprs: &HashMap<HirTypeId, ReprId>,
) -> Option<Vec<ValueShape>> {
    if constructor.field_count == 0 {
        return Some(Vec::new());
    }
    let substituted = match (name, arguments.len()) {
        (Some("Data.Maybe.Maybe"), 1) => Some(arguments[0]),
        (Some("Data.Either.Either"), 2) => {
            Some(arguments[if constructor.tag == 0 { 0 } else { 1 }])
        }
        _ => None,
    };
    if let Some(argument) = substituted {
        return Some(vec![payload_shape(
            module,
            argument,
            record_types,
            array_types,
            type_reprs,
        )?]);
    }
    constructor
        .field_types
        .iter()
        .map(|field| payload_shape(module, *field, record_types, array_types, type_reprs))
        .collect()
}

fn application_arguments(module: &CoreModule, id: TypeId) -> Vec<TypeId> {
    let mut arguments = Vec::new();
    let mut current = id;
    while let Some(CoreType::Application(function, argument)) = module.types.get(current.0 as usize)
    {
        arguments.push(*argument);
        current = *function;
    }
    arguments.reverse();
    arguments
}

fn user_type_name(module: &CoreModule, head: HirTypeId) -> Option<&str> {
    module
        .type_names
        .iter()
        .find(|(candidate, _)| *candidate == head)
        .map(|(_, name)| name.as_str())
}

fn is_array(module: &CoreModule, id: TypeId) -> bool {
    matches!(
        module.types.get(id.0 as usize),
        Some(CoreType::Constructor(TypeConstructor::Array))
    )
}

fn erased_shape() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

/// Maps each payload-bearing data type declaration to the variant representation
/// the `TypeLayout` assigned to its constructors. Nullary enums, opaque handles,
/// and newtypes are absent.
fn type_representations(
    module: &CoreModule,
    constructor_types: &HashMap<SymbolId, ReprId>,
) -> HashMap<HirTypeId, ReprId> {
    module
        .constructors
        .iter()
        .filter_map(|constructor| {
            constructor_types
                .get(&constructor.symbol)
                .map(|repr| (constructor.type_id, *repr))
        })
        .collect()
}

fn payload_shape(
    module: &CoreModule,
    id: TypeId,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    type_reprs: &HashMap<HirTypeId, ReprId>,
) -> Option<ValueShape> {
    Some(match module.types.get(id.0 as usize)? {
        CoreType::I32 | CoreType::Char | CoreType::Unit => ValueShape::Integer,
        CoreType::Boolean => ValueShape::Boolean,
        CoreType::F64 => ValueShape::Number,
        CoreType::String => ValueShape::String,
        CoreType::Record(_) => reference(*record_types.get(&id)?),
        CoreType::Application(function, _)
            if matches!(
                module.types.get(function.0 as usize),
                Some(CoreType::Constructor(TypeConstructor::Array))
            ) =>
        {
            reference(*array_types.get(&id)?)
        }
        CoreType::Constructor(TypeConstructor::User(_)) | CoreType::Application(_, _) => {
            // A newtype is represented by its single field, so `Resource a`
            // erases to the `Int` handle index it wraps (DEC-14).
            match newtype_underlying(module, id) {
                Some(inner) => payload_shape(module, inner, record_types, array_types, type_reprs)?,
                None => user_payload_shape(module, id, type_reprs)?,
            }
        }
        _ => return None,
    })
}

/// The single field type of a newtype at its resolved application, or `None`
/// when the type is not a newtype.
fn newtype_underlying(module: &CoreModule, id: TypeId) -> Option<TypeId> {
    let head = head_user_type(module, id)?;
    super::layout::newtype_field_type(module, head)
}

/// The abstract shape of a source data type: an `i32` for a nullary enum or an
/// opaque handle, otherwise a reference to its variant representation.
fn user_payload_shape(
    module: &CoreModule,
    id: TypeId,
    type_reprs: &HashMap<HirTypeId, ReprId>,
) -> Option<ValueShape> {
    let head = head_user_type(module, id)?;
    let constructors = constructors_of(module, head);
    if constructors.is_empty() || constructors.iter().all(|case| case.field_count == 0) {
        return Some(ValueShape::Integer);
    }
    Some(reference(*type_reprs.get(&head)?))
}

fn head_user_type(module: &CoreModule, mut id: TypeId) -> Option<HirTypeId> {
    loop {
        match module.types.get(id.0 as usize)? {
            CoreType::Constructor(TypeConstructor::User(type_id)) => return Some(*type_id),
            CoreType::Application(function, _) => id = *function,
            _ => return None,
        }
    }
}

fn constructors_of(module: &CoreModule, type_id: HirTypeId) -> Vec<&psrs_core::ConstructorInfo> {
    let mut constructors = module
        .constructors
        .iter()
        .filter(|constructor| constructor.type_id == type_id)
        .collect::<Vec<_>>();
    constructors.sort_by_key(|constructor| constructor.tag);
    constructors
}

fn reference(representation: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(representation),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_core::ConstructorInfo;
    use psrs_hir::{ModuleId, SymbolId, TypeVariableId};

    fn reference(repr: ReprId) -> ValueShape {
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(repr),
        })
    }

    /// `result<list<u8>, stream-error>` maps to `Either String StreamError`.
    /// The result's decode tree must name the nested `StreamError` reference for
    /// the err case even though the `Either` representation stores it erased.
    #[test]
    fn result_decode_tree_resolves_a_nested_variant_payload() {
        let either = HirTypeId::new(ModuleId(0), 0);
        let stream_error = HirTypeId::new(ModuleId(0), 1);
        let error = HirTypeId::new(ModuleId(0), 2);
        let left = SymbolId::new(ModuleId(0), 0);
        let right = SymbolId::new(ModuleId(0), 1);
        let failed = SymbolId::new(ModuleId(0), 2);
        let closed = SymbolId::new(ModuleId(0), 3);
        let variable = TypeId(2);
        let stream_error_ty = TypeId(0);
        let error_ty = TypeId(1);
        let string = TypeId(3);
        let either_ctor = TypeId(4);
        let either_string = TypeId(5);
        let result = TypeId(6);
        let module = CoreModule {
            type_names: vec![
                (either, "Data.Either.Either".into()),
                (stream_error, "WASI.Streams.StreamError".into()),
            ],
            id: ModuleId(0),
            name: "ResultDecode".into(),
            externals: Vec::new(),
            types: vec![
                CoreType::Constructor(TypeConstructor::User(stream_error)),
                CoreType::Constructor(TypeConstructor::User(error)),
                CoreType::Variable(TypeVariableId(0)),
                CoreType::String,
                CoreType::Constructor(TypeConstructor::User(either)),
                CoreType::Application(either_ctor, string),
                CoreType::Application(either_string, stream_error_ty),
            ],
            newtype_ids: Vec::new(),
            opaque_ids: vec![error],
            constructors: vec![
                ConstructorInfo {
                    symbol: left,
                    name: "Left".into(),
                    type_id: either,
                    tag: 0,
                    field_count: 1,
                    field_types: vec![variable],
                },
                ConstructorInfo {
                    symbol: right,
                    name: "Right".into(),
                    type_id: either,
                    tag: 1,
                    field_count: 1,
                    field_types: vec![variable],
                },
                ConstructorInfo {
                    symbol: failed,
                    name: "LastOperationFailed".into(),
                    type_id: stream_error,
                    tag: 0,
                    field_count: 1,
                    field_types: vec![error_ty],
                },
                ConstructorInfo {
                    symbol: closed,
                    name: "Closed".into(),
                    type_id: stream_error,
                    tag: 1,
                    field_count: 0,
                    field_types: Vec::new(),
                },
            ],
            declarations: Vec::new(),
            entry: None,
            span: psrs_span::TextRange::new(0, 0),
        };
        let constructor_types = [
            (left, ReprId(0)),
            (right, ReprId(0)),
            (failed, ReprId(1)),
            (closed, ReprId(1)),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>();
        let guest = abstract_result_guest(
            Some(result),
            &module,
            &HashMap::new(),
            &HashMap::new(),
            &constructor_types,
        )
        .expect("the Either result should have a decode layout");
        let GuestLayout::Variant { repr, cases } = guest else {
            panic!("the decode layout should be a variant, got {guest:?}");
        };
        assert_eq!(repr, ReprId(0));
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].fields, vec![ValueShape::String]);
        assert_eq!(cases[1].fields, vec![reference(ReprId(1))]);
    }
}

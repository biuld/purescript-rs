use super::{
    function_type_signature, is_callable_type, layout_error, newtype_field_type,
    primitive_shape_of, unquantified_type, user_type_id,
};
use crate::BackendError;
use crate::cc::{RefShape, Reference, ReprId, Signature, SignatureId, ValueShape};
use psrs_core::{Module as CoreModule, Type, TypeConstructor, TypeId};
use psrs_hir::TypeId as HirTypeId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[allow(clippy::too_many_arguments)]
pub(crate) fn declaration_shape(
    declaration: &psrs_core::Declaration,
    module: &CoreModule,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, ReprId>,
    record_types: &HashMap<TypeId, ReprId>,
    function_types: &HashMap<TypeId, SignatureId>,
) -> Result<Signature, Vec<BackendError>> {
    let call = super::declaration_call_parts(module, declaration)?;
    let ty = call.result;
    let parameters = call
        .parameters
        .into_iter()
        .map(|(ty, span)| {
            scalar_type(
                module,
                ty,
                span,
                enum_types,
                aggregate_types,
                newtype_ids,
                array_types,
                record_types,
                function_types,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if is_callable_type(module, ty) {
        return Ok(Signature {
            parameters,
            result: callable_value_shape(module, ty, declaration.span, function_types)?,
        });
    }
    if let Some(shape) = primitive_shape_of(module, ty) {
        return Ok(Signature {
            parameters,
            result: shape,
        });
    }
    match module.types.get(ty.0 as usize) {
        Some(_) if is_abstract_type(module, ty) => Ok(Signature {
            parameters,
            result: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            }),
        }),
        Some(Type::Constructor(TypeConstructor::User(id))) if enum_types.contains(id) => {
            Ok(Signature {
                parameters,
                result: ValueShape::Integer,
            })
        }
        Some(Type::Constructor(TypeConstructor::User(id))) if aggregate_types.contains(id) => {
            Ok(Signature {
                parameters,
                result: aggregate_value_type(),
            })
        }
        // An opaque `foreign import data` type is a nominal resource handle: a
        // single `i32` table index. DEC-14 makes it the source representation of
        // an `own`/`borrow` handle.
        Some(Type::Constructor(TypeConstructor::User(id))) if module.opaque_ids.contains(id) => {
            Ok(Signature {
                parameters,
                result: ValueShape::Integer,
            })
        }
        Some(_) if module.is_record_type(ty) => {
            if module.record_is_open(ty).unwrap_or(false) {
                Err(vec![BackendError::new(
                    "P8 closure conversion",
                    declaration.span,
                    "open record rows have no runtime layout",
                )])
            } else if let Some(repr) = record_types.get(&ty) {
                Ok(Signature {
                    parameters,
                    result: ValueShape::Reference(Reference {
                        nullable: false,
                        heap: RefShape::Repr(*repr),
                    }),
                })
            } else {
                Err(vec![BackendError::new(
                    "P8 closure conversion",
                    declaration.span,
                    "record type has no representation requirement",
                )])
            }
        }
        Some(Type::Application(_, _)) => {
            if let Some(array_repr) = array_types.get(&ty) {
                return Ok(Signature {
                    parameters,
                    result: ValueShape::Reference(Reference {
                        nullable: false,
                        heap: RefShape::Repr(*array_repr),
                    }),
                });
            }
            let Some(type_id) = user_type_id(module, ty) else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    declaration.span,
                    "the first backend slice cannot represent aggregate or parameterized types",
                )]);
            };
            if module.opaque_ids.contains(&type_id) {
                // An applied foreign type is not a nullary handle. It has no
                // constructors, so its value uses the erased reference until a
                // later calling convention gives that type its own layout.
                Ok(Signature {
                    parameters,
                    result: erased_reference(),
                })
            } else if enum_types.contains(&type_id) {
                Ok(Signature {
                    parameters,
                    result: ValueShape::Integer,
                })
            } else if aggregate_types.contains(&type_id) {
                Ok(Signature {
                    parameters,
                    result: aggregate_value_type(),
                })
            } else if newtype_ids.contains(&type_id) {
                let Some(inner) = newtype_field_type(module, type_id) else {
                    return Err(layout_error(
                        declaration.span,
                        "newtype must have exactly one field",
                    ));
                };
                Ok(Signature {
                    parameters,
                    result: scalar_type(
                        module,
                        inner,
                        declaration.span,
                        enum_types,
                        aggregate_types,
                        newtype_ids,
                        array_types,
                        record_types,
                        function_types,
                    )?,
                })
            } else {
                Err(vec![BackendError::new(
                    "P8 closure conversion",
                    declaration.span,
                    "the first backend slice cannot represent aggregate or parameterized types",
                )])
            }
        }
        Some(Type::Constructor(TypeConstructor::User(type_id)))
            if newtype_ids.contains(type_id) =>
        {
            Ok(Signature {
                parameters,
                result: scalar_type(
                    module,
                    ty,
                    declaration.span,
                    enum_types,
                    aggregate_types,
                    newtype_ids,
                    array_types,
                    record_types,
                    function_types,
                )?,
            })
        }
        Some(Type::Variable(_))
        | Some(Type::Constructor(_))
        | Some(Type::ForAll { .. })
        | Some(Type::Closure { .. })
        | Some(Type::RowEmpty)
        | Some(Type::TypeLevelString(_))
        | Some(Type::TypeLevelInt(_))
        | Some(Type::RowExtend { .. }) => Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.span,
            "the first backend slice cannot represent aggregate or parameterized types",
        )]),
        None => Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.span,
            "declaration type is outside the Core type table",
        )]),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn scalar_type(
    module: &CoreModule,
    id: psrs_core::TypeId,
    span: TextRange,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, ReprId>,
    record_types: &HashMap<TypeId, ReprId>,
    function_types: &HashMap<TypeId, SignatureId>,
) -> Result<ValueShape, Vec<BackendError>> {
    if primitive_shape_of(module, id) == Some(ValueShape::State) {
        return Ok(ValueShape::State);
    }
    // A callable closure value — an ordinary arrow or a registered callable
    // constructor application — is represented by its closure signature.
    if is_callable_type(module, id) {
        return callable_value_shape(module, id, span, function_types);
    }
    let id = unquantified_type(module, id);
    if let Some(shape) = primitive_shape_of(module, id) {
        return Ok(shape);
    }
    match module.types.get(id.0 as usize) {
        Some(_) if is_abstract_type(module, id) => Ok(ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        })),
        Some(Type::Constructor(TypeConstructor::User(id))) if enum_types.contains(id) => {
            Ok(ValueShape::Integer)
        }
        Some(Type::Constructor(TypeConstructor::User(id))) if aggregate_types.contains(id) => {
            Ok(aggregate_value_type())
        }
        // An opaque resource handle is one `i32` table index (DEC-14).
        Some(Type::Constructor(TypeConstructor::User(id))) if module.opaque_ids.contains(id) => {
            Ok(ValueShape::Integer)
        }
        Some(Type::Application(_, _)) if array_types.contains_key(&id) => {
            Ok(ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(array_types[&id]),
            }))
        }
        Some(_) if module.is_record_type(id) => {
            if module.record_is_open(id).unwrap_or(false) {
                Err(vec![BackendError::new(
                    "P8 closure conversion",
                    span,
                    "open record rows have no runtime layout",
                )])
            } else if let Some(repr) = record_types.get(&id) {
                Ok(ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(*repr),
                }))
            } else {
                Err(vec![BackendError::new(
                    "P8 closure conversion",
                    span,
                    "record type has no representation requirement",
                )])
            }
        }
        Some(Type::Application(_, _)) => {
            let Some(type_id) = user_type_id(module, id) else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    span,
                    "aggregate and parameterized types are not supported by the first backend slice",
                )]);
            };
            if module.opaque_ids.contains(&type_id) {
                return Ok(erased_reference());
            }
            if enum_types.contains(&type_id) {
                Ok(ValueShape::Integer)
            } else if aggregate_types.contains(&type_id) {
                Ok(aggregate_value_type())
            } else if newtype_ids.contains(&type_id) {
                let Some(inner) = newtype_field_type(module, type_id) else {
                    return Err(layout_error(span, "newtype must have exactly one field"));
                };
                scalar_type(
                    module,
                    inner,
                    span,
                    enum_types,
                    aggregate_types,
                    newtype_ids,
                    array_types,
                    record_types,
                    function_types,
                )
            } else {
                Err(vec![BackendError::new(
                    "P8 closure conversion",
                    span,
                    "aggregate and parameterized types are not supported by the first backend slice",
                )])
            }
        }
        Some(Type::Constructor(TypeConstructor::User(type_id)))
            if newtype_ids.contains(type_id) =>
        {
            let Some(inner) = newtype_field_type(module, *type_id) else {
                return Err(layout_error(span, "newtype must have exactly one field"));
            };
            scalar_type(
                module,
                inner,
                span,
                enum_types,
                aggregate_types,
                newtype_ids,
                array_types,
                record_types,
                function_types,
            )
        }
        Some(Type::Variable(_))
        | Some(Type::Constructor(_))
        | Some(Type::ForAll { .. })
        | Some(Type::Closure { .. })
        | Some(Type::RowEmpty)
        | Some(Type::TypeLevelString(_))
        | Some(Type::TypeLevelInt(_))
        | Some(Type::RowExtend { .. }) => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "aggregate and parameterized types are not supported by the first backend slice",
        )]),
        None => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "expression type is outside the Core type table",
        )]),
    }
}

fn erased_reference() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

fn aggregate_value_type() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    })
}

fn closure_value_type_for(signature: SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

/// The runtime shape of a callable closure value — an ordinary arrow or a
/// registered callable constructor application. It retains its normalized
/// signature, including erased abstract arguments and results. The hidden
/// context parameter lives only in the signature, never in this value shape.
fn callable_value_shape(
    module: &CoreModule,
    id: TypeId,
    span: TextRange,
    function_types: &HashMap<TypeId, SignatureId>,
) -> Result<ValueShape, Vec<BackendError>> {
    let Some(signature) = function_type_signature(module, function_types, id) else {
        return Err(layout_error(span, "callable value has no runtime layout"));
    };
    Ok(closure_value_type_for(signature))
}

/// Bare variables and applications headed by a variable have no known storage
/// constructor. Their instantiated value crosses the uniform erased protocol.
pub(in crate::cc) fn is_abstract_type(module: &CoreModule, mut id: TypeId) -> bool {
    id = unquantified_type(module, id);
    for _ in 0..module.types.len() {
        match module.types.get(id.0 as usize) {
            Some(Type::Variable(_)) => return true,
            Some(Type::Application(function, _)) => id = *function,
            _ => return false,
        }
    }
    false
}

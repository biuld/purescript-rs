use super::{depends_on_type_variable, layout_error, newtype_field_type, user_type_id};
use crate::BackendError;
use crate::cc::{RefShape, Reference, ReprId, Signature, SignatureId, ValueShape};
use psrs_core::ExprKind;
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
    let mut ty = declaration.ty;
    let mut parameters = Vec::new();
    let mut value = &declaration.value;
    while let ExprKind::Lambda { binder, body } = &value.kind {
        let Some(Type::Function { parameter, result }) = module.types.get(ty.0 as usize) else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                binder.span,
                "lambda binder does not have a function type",
            )]);
        };
        if *parameter != binder.ty {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                binder.span,
                "lambda binder type differs from the function parameter type",
            )]);
        }
        ty = *result;
        parameters.push(scalar_type(
            module,
            binder.ty,
            binder.span,
            enum_types,
            aggregate_types,
            newtype_ids,
            array_types,
            record_types,
            function_types,
        )?);
        value = body;
    }
    match module.types.get(ty.0 as usize) {
        Some(Type::I32 | Type::Char | Type::String | Type::Unit) => Ok(Signature {
            parameters,
            result: ValueShape::Integer,
        }),
        Some(Type::F64) => Ok(Signature {
            parameters,
            result: ValueShape::Number,
        }),
        Some(Type::Boolean) => Ok(Signature {
            parameters,
            result: ValueShape::Boolean,
        }),
        Some(Type::Variable(_)) => Ok(Signature {
            parameters,
            result: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            }),
        }),
        Some(Type::Function { .. }) => Ok(Signature {
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
        Some(Type::Record(_)) if record_types.contains_key(&ty) => Ok(Signature {
            parameters,
            result: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(record_types[&ty]),
            }),
        }),
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
            if enum_types.contains(&type_id) {
                Ok(Signature {
                    parameters,
                    result: ValueShape::Integer,
                })
            } else if aggregate_types.contains(&type_id) {
                Ok(Signature {
                    parameters,
                    result: aggregate_value_type(),
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
        Some(Type::Constructor(_)) => Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.span,
            "the first backend slice cannot represent aggregate or parameterized types",
        )]),
        Some(Type::Record(_)) => Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.span,
            "record type has no representation requirement",
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
    match module.types.get(id.0 as usize) {
        Some(Type::I32 | Type::Char | Type::String | Type::Unit) => Ok(ValueShape::Integer),
        Some(Type::F64) => Ok(ValueShape::Number),
        Some(Type::Boolean) => Ok(ValueShape::Boolean),
        Some(Type::Variable(_)) => Ok(ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        })),
        Some(Type::Function { .. }) => {
            if !function_types.contains_key(&id) {
                return Err(layout_error(span, "function type has no runtime layout"));
            }
            if depends_on_type_variable(module, id) {
                Ok(ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Erased,
                }))
            } else {
                Ok(closure_value_type_for(function_types[&id]))
            }
        }
        Some(Type::Constructor(TypeConstructor::User(id))) if enum_types.contains(id) => {
            Ok(ValueShape::Integer)
        }
        Some(Type::Constructor(TypeConstructor::User(id))) if aggregate_types.contains(id) => {
            Ok(aggregate_value_type())
        }
        Some(Type::Application(_, _)) if array_types.contains_key(&id) => {
            Ok(ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(array_types[&id]),
            }))
        }
        Some(Type::Record(_)) if record_types.contains_key(&id) => {
            Ok(ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(record_types[&id]),
            }))
        }
        Some(Type::Application(_, _)) => {
            let Some(type_id) = user_type_id(module, id) else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    span,
                    "aggregate and parameterized types are not supported by the first backend slice",
                )]);
            };
            if enum_types.contains(&type_id) {
                Ok(ValueShape::Integer)
            } else if aggregate_types.contains(&type_id) {
                Ok(aggregate_value_type())
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
        Some(Type::Constructor(_)) => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "aggregate and parameterized types are not supported by the first backend slice",
        )]),
        Some(Type::Record(_)) => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "record type has no representation requirement",
        )]),
        None => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "expression type is outside the Core type table",
        )]),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn field_storage_shape(
    module: &CoreModule,
    id: TypeId,
    span: TextRange,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, ReprId>,
    record_types: &HashMap<TypeId, ReprId>,
    function_types: &HashMap<TypeId, SignatureId>,
) -> Result<ValueShape, Vec<BackendError>> {
    if depends_on_type_variable(module, id) {
        return Ok(ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        }));
    }
    scalar_type(
        module,
        id,
        span,
        enum_types,
        aggregate_types,
        newtype_ids,
        array_types,
        record_types,
        function_types,
    )
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

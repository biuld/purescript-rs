use super::*;
use crate::types::FunctionSignature;
use psrs_core::ExprKind;

#[allow(clippy::too_many_arguments)]
pub(crate) fn declaration_shape(
    declaration: &psrs_core::Declaration,
    module: &CoreModule,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, u32>,
    record_types: &HashMap<TypeId, u32>,
    function_types: &HashMap<TypeId, u32>,
) -> Result<FunctionSignature, Vec<BackendError>> {
    if !declaration.quantified.is_empty() {
        return Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.name_span,
            "polymorphic declarations are not supported by the first backend slice",
        )]);
    }
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
        if matches!(
            module.types.get(binder.ty.0 as usize),
            Some(Type::Variable(_))
        ) {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                declaration.name_span,
                "polymorphic declarations are not supported by the first backend slice",
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
        Some(Type::I32 | Type::String | Type::Unit) => Ok(FunctionSignature {
            parameters,
            result: ValueType::I32,
        }),
        Some(Type::Boolean) => Ok(FunctionSignature {
            parameters,
            result: ValueType::Boolean,
        }),
        Some(Type::Variable(_)) => Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.name_span,
            "polymorphic declarations are not supported by the first backend slice",
        )]),
        Some(Type::Function { .. }) => Ok(FunctionSignature {
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
            Ok(FunctionSignature {
                parameters,
                result: ValueType::I32,
            })
        }
        Some(Type::Constructor(TypeConstructor::User(id))) if aggregate_types.contains(id) => {
            Ok(FunctionSignature {
                parameters,
                result: aggregate_value_type(),
            })
        }
        Some(Type::Record(_)) if record_types.contains_key(&ty) => Ok(FunctionSignature {
            parameters,
            result: aggregate_value_type_for(record_types[&ty]),
        }),
        Some(Type::Application(_, _)) => {
            if let Some(type_index) = array_types.get(&ty) {
                return Ok(FunctionSignature {
                    parameters,
                    result: ValueType::Ref(RefType {
                        nullable: false,
                        heap: HeapType::Index(*type_index),
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
                Ok(FunctionSignature {
                    parameters,
                    result: ValueType::I32,
                })
            } else if aggregate_types.contains(&type_id) {
                Ok(FunctionSignature {
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
            Ok(FunctionSignature {
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
            "record type has no concrete GC struct layout",
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
    array_types: &HashMap<TypeId, u32>,
    record_types: &HashMap<TypeId, u32>,
    function_types: &HashMap<TypeId, u32>,
) -> Result<ValueType, Vec<BackendError>> {
    match module.types.get(id.0 as usize) {
        Some(Type::I32 | Type::String | Type::Unit) => Ok(ValueType::I32),
        Some(Type::Boolean) => Ok(ValueType::Boolean),
        Some(Type::Variable(_)) => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "polymorphic values are not supported by the first backend slice",
        )]),
        Some(Type::Function { .. }) => {
            if !function_types.contains_key(&id) {
                return Err(layout_error(span, "function type has no runtime layout"));
            }
            Ok(aggregate_value_type())
        }
        Some(Type::Constructor(TypeConstructor::User(id))) if enum_types.contains(id) => {
            Ok(ValueType::I32)
        }
        Some(Type::Constructor(TypeConstructor::User(id))) if aggregate_types.contains(id) => {
            Ok(aggregate_value_type())
        }
        Some(Type::Application(_, _)) if array_types.contains_key(&id) => {
            Ok(ValueType::Ref(RefType {
                nullable: false,
                heap: HeapType::Index(array_types[&id]),
            }))
        }
        Some(Type::Record(_)) if record_types.contains_key(&id) => {
            Ok(aggregate_value_type_for(record_types[&id]))
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
                Ok(ValueType::I32)
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
            "record type has no concrete GC struct layout",
        )]),
        None => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "expression type is outside the Core type table",
        )]),
    }
}

fn aggregate_value_type() -> ValueType {
    ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Struct,
    })
}

fn aggregate_value_type_for(type_index: u32) -> ValueType {
    ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(type_index),
    })
}

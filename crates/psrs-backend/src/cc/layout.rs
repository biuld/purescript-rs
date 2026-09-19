use super::ValueType;
use crate::BackendError;
use crate::types::{
    CompositeType, DefinedType, FieldType, FunctionSignature, HeapType, RecGroup, RefType,
    StorageType,
};
use psrs_core::{Module as CoreModule, Type, TypeConstructor, TypeId};
use psrs_hir::{ExternalKind, ExternalSymbol, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod functions;
mod scalar;

pub(crate) use functions::function_signature;
pub(super) use scalar::{declaration_shape, scalar_type};

pub(super) type Signature = FunctionSignature;

pub(super) fn runtime_signature(external: &ExternalSymbol) -> Option<Signature> {
    match &external.kind {
        ExternalKind::Wit { .. } => declared_signature(external.signature.as_ref()?),
        ExternalKind::Intrinsic(_) => None,
    }
}

/// The arity and result scalar of a value declared with a WIT binding. The
/// declared type's arrows become the call arity; the result must be a scalar the
/// first backend slice can represent.
fn declared_signature(signature: &psrs_hir::Type) -> Option<Signature> {
    let mut ty = signature;
    let mut parameters = Vec::new();
    while let psrs_hir::TypeKind::Function { result, .. } = &ty.kind {
        let parameter = match &ty.kind {
            psrs_hir::TypeKind::Function { parameter, .. } => match &parameter.kind {
                psrs_hir::TypeKind::Constructor(psrs_hir::BuiltinType::Boolean) => {
                    ValueType::Boolean
                }
                psrs_hir::TypeKind::Constructor(psrs_hir::BuiltinType::Number) => ValueType::F64,
                psrs_hir::TypeKind::Constructor(
                    psrs_hir::BuiltinType::Int
                    | psrs_hir::BuiltinType::String
                    | psrs_hir::BuiltinType::Unit,
                ) => ValueType::I32,
                _ => return None,
            },
            _ => return None,
        };
        parameters.push(parameter);
        ty = result;
    }
    let result = match &ty.kind {
        psrs_hir::TypeKind::Constructor(psrs_hir::BuiltinType::Boolean) => ValueType::Boolean,
        psrs_hir::TypeKind::Constructor(
            psrs_hir::BuiltinType::Int
            | psrs_hir::BuiltinType::String
            | psrs_hir::BuiltinType::Unit,
        ) => ValueType::I32,
        psrs_hir::TypeKind::Constructor(psrs_hir::BuiltinType::Number) => ValueType::F64,
        _ => return None,
    };
    Some(Signature { parameters, result })
}

/// The set of user types whose constructors are all nullary, which the first
/// runtime slice can represent as immediate integer tags.
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

/// User types with at least one field are represented by immutable GC structs.
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

/// Builds one concrete struct type per constructor. Field zero stores the
/// constructor tag; the remaining fields use the source field layout. Values
/// of an aggregate user type use the abstract `struct` reference, while case
/// lowering casts them to the concrete constructor type.
pub(super) fn type_layout(
    module: &CoreModule,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
) -> Result<TypeLayout, Vec<BackendError>> {
    let mut definitions = Vec::new();
    let mut constructor_types = HashMap::new();
    let boxed_i32_type = if module
        .constructors
        .iter()
        .flat_map(|constructor| &constructor.field_types)
        .any(|field| depends_on_type_variable(module, *field))
    {
        let type_index = definitions.len() as u32;
        definitions.push(DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Struct(vec![FieldType {
                storage: StorageType::I32,
                mutable: false,
            }]),
        });
        Some(type_index)
    } else {
        None
    };
    let boxed_f64_type = if module.types.iter().any(|ty| matches!(ty, Type::F64)) {
        let type_index = definitions.len() as u32;
        definitions.push(DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Struct(vec![FieldType {
                storage: StorageType::F64,
                mutable: false,
            }]),
        });
        Some(type_index)
    } else {
        None
    };
    // Reserve all array and record indices before constructing either kind.
    // Their storage types may refer to each other (for example, an array of
    // records or a record containing an array), so assigning indices while
    // walking one family would make the result depend on declaration order.
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
    let array_base = definitions.len() as u32;
    for (offset, id) in array_ids.iter().enumerate() {
        array_types.insert(*id, array_base + offset as u32);
    }

    let record_ids = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, ty)| matches!(ty, Type::Record(_)).then_some(TypeId(index as u32)))
        .collect::<Vec<_>>();
    let record_base = array_base + array_ids.len() as u32;
    let mut record_types = HashMap::new();
    for (offset, id) in record_ids.iter().enumerate() {
        record_types.insert(*id, record_base + offset as u32);
    }

    for id in &array_ids {
        let Some(element) = array_element_type(module, *id) else {
            continue;
        };
        let storage = storage_type(
            module,
            element,
            aggregate_types,
            newtype_ids,
            &array_types,
            &record_types,
            module.span,
        )?;
        definitions.push(DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Array(FieldType {
                storage,
                mutable: true,
            }),
        });
    }
    for id in &record_ids {
        let Type::Record(fields) = &module.types[id.0 as usize] else {
            continue;
        };
        let mut layout_fields = Vec::with_capacity(fields.len());
        for (_, field) in fields {
            layout_fields.push(FieldType {
                storage: storage_type(
                    module,
                    *field,
                    aggregate_types,
                    newtype_ids,
                    &array_types,
                    &record_types,
                    module.span,
                )?,
                mutable: false,
            });
        }
        definitions.push(DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Struct(layout_fields),
        });
    }
    for constructor in &module.constructors {
        if !aggregate_types.contains(&constructor.type_id) {
            continue;
        }
        if constructor.field_types.len() != constructor.field_count {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                module.span,
                "constructor field metadata is inconsistent",
            )]);
        }
        let mut fields = vec![FieldType {
            storage: StorageType::I32,
            mutable: false,
        }];
        for field in &constructor.field_types {
            fields.push(FieldType {
                storage: storage_type(
                    module,
                    *field,
                    aggregate_types,
                    newtype_ids,
                    &array_types,
                    &record_types,
                    module.span,
                )?,
                mutable: false,
            });
        }
        let type_index = definitions.len() as u32;
        constructor_types.insert(constructor.symbol, type_index);
        definitions.push(DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Struct(fields),
        });
    }
    let function_layout = functions::append_function_types(
        module,
        enum_types,
        aggregate_types,
        newtype_ids,
        &array_types,
        &record_types,
        &mut definitions,
    )?;
    let types = if definitions.is_empty() {
        Vec::new()
    } else {
        vec![RecGroup(definitions)]
    };
    Ok(TypeLayout {
        types,
        array_types,
        record_types,
        constructor_types,
        boxed_i32_type,
        boxed_f64_type,
        function_types: function_layout.function_types,
        capture_array_type: function_layout.capture_array_type,
        closure_type: function_layout.closure_type,
    })
}

pub(super) struct TypeLayout {
    pub(super) types: Vec<RecGroup>,
    pub(super) array_types: HashMap<TypeId, u32>,
    pub(super) record_types: HashMap<TypeId, u32>,
    pub(super) constructor_types: HashMap<SymbolId, u32>,
    pub(super) boxed_i32_type: Option<u32>,
    pub(super) boxed_f64_type: Option<u32>,
    pub(super) function_types: HashMap<TypeId, u32>,
    pub(super) capture_array_type: Option<u32>,
    pub(super) closure_type: Option<u32>,
}

fn storage_type(
    module: &CoreModule,
    id: psrs_core::TypeId,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, u32>,
    record_types: &HashMap<TypeId, u32>,
    span: TextRange,
) -> Result<StorageType, Vec<BackendError>> {
    if depends_on_type_variable(module, id) {
        return Ok(StorageType::Ref(RefType {
            nullable: false,
            heap: HeapType::Eq,
        }));
    }
    match module.types.get(id.0 as usize) {
        Some(Type::I32 | Type::Boolean | Type::Char | Type::String | Type::Unit) => {
            Ok(StorageType::I32)
        }
        Some(Type::F64) => Ok(StorageType::F64),
        Some(Type::Constructor(TypeConstructor::User(type_id)))
            if newtype_ids.contains(type_id) =>
        {
            let Some(inner) = newtype_field_type(module, *type_id) else {
                return Err(layout_error(span, "newtype must have exactly one field"));
            };
            storage_type(
                module,
                inner,
                aggregate_types,
                newtype_ids,
                array_types,
                record_types,
                span,
            )
        }
        Some(Type::Constructor(TypeConstructor::User(type_id)))
            if aggregate_types.contains(type_id) =>
        {
            Ok(StorageType::Ref(RefType {
                nullable: false,
                heap: HeapType::Struct,
            }))
        }
        Some(Type::Constructor(TypeConstructor::User(_))) => Ok(StorageType::I32),
        Some(Type::Application(_, _)) if array_types.contains_key(&id) => {
            Ok(StorageType::Ref(RefType {
                nullable: false,
                heap: HeapType::Index(array_types[&id]),
            }))
        }
        Some(Type::Record(_)) if record_types.contains_key(&id) => Ok(StorageType::Ref(RefType {
            nullable: false,
            heap: HeapType::Index(record_types[&id]),
        })),
        Some(Type::Variable(_))
        | Some(Type::Constructor(TypeConstructor::Array))
        | Some(Type::Application(_, _))
        | Some(Type::Record(_))
        | Some(Type::Function { .. })
        | None => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "constructor fields must have monomorphic scalar or aggregate types",
        )]),
    }
}

fn newtype_field_type(module: &CoreModule, type_id: HirTypeId) -> Option<TypeId> {
    let constructors = module
        .constructors
        .iter()
        .filter(|constructor| constructor.type_id == type_id);
    let mut constructors = constructors;
    let constructor = constructors.next()?;
    if constructors.next().is_some() || constructor.field_count != 1 {
        return None;
    }
    constructor.field_types.first().copied()
}

fn layout_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
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
            _ => false,
        };
        visiting.remove(&id);
        result
    }

    visit(module, id, &mut HashSet::new())
}

use super::ValueType;
use crate::BackendError;
use crate::types::{
    CompositeType, DefinedType, FieldType, HeapType, RecGroup, RefType, StorageType,
};
use psrs_core::{ExprKind, Module as CoreModule, Type, TypeConstructor, TypeId};
use psrs_hir::{ExternalKind, ExternalSymbol, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
pub(super) struct Signature {
    pub(super) arity: usize,
    pub(super) result: ValueType,
}

pub(super) fn runtime_signature(external: &ExternalSymbol) -> Option<Signature> {
    match &external.kind {
        ExternalKind::Wit { .. } => {
            let (arity, result) = declared_signature(external.signature.as_ref()?)?;
            Some(Signature { arity, result })
        }
        ExternalKind::Intrinsic(_) => None,
    }
}

/// The arity and result scalar of a value declared with a WIT binding. The
/// declared type's arrows become the call arity; the result must be a scalar the
/// first backend slice can represent.
fn declared_signature(signature: &psrs_hir::Type) -> Option<(usize, ValueType)> {
    let mut ty = signature;
    let mut arity = 0;
    while let psrs_hir::TypeKind::Function { result, .. } = &ty.kind {
        arity += 1;
        ty = result;
    }
    let result = match &ty.kind {
        psrs_hir::TypeKind::Constructor(psrs_hir::BuiltinType::Boolean) => ValueType::Boolean,
        psrs_hir::TypeKind::Constructor(
            psrs_hir::BuiltinType::Int
            | psrs_hir::BuiltinType::String
            | psrs_hir::BuiltinType::Unit,
        ) => ValueType::I32,
        _ => return None,
    };
    Some((arity, result))
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
        Some(Type::I32 | Type::Boolean | Type::String | Type::Unit) => true,
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
        | Some(Type::Application(_, _))
        | Some(Type::Function { .. })
        | None => false,
    }
}

/// Builds one concrete struct type per constructor. Field zero stores the
/// constructor tag; the remaining fields use the source field layout. Values
/// of an aggregate user type use the abstract `struct` reference, while case
/// lowering casts them to the concrete constructor type.
pub(super) fn type_layout(
    module: &CoreModule,
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
                storage: storage_type(module, *field, aggregate_types, newtype_ids, module.span)?,
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
    let types = if definitions.is_empty() {
        Vec::new()
    } else {
        vec![RecGroup(definitions)]
    };
    Ok(TypeLayout {
        types,
        constructor_types,
        boxed_i32_type,
    })
}

pub(super) struct TypeLayout {
    pub(super) types: Vec<RecGroup>,
    pub(super) constructor_types: HashMap<SymbolId, u32>,
    pub(super) boxed_i32_type: Option<u32>,
}

fn storage_type(
    module: &CoreModule,
    id: psrs_core::TypeId,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    span: TextRange,
) -> Result<StorageType, Vec<BackendError>> {
    if depends_on_type_variable(module, id) {
        return Ok(StorageType::Ref(RefType {
            nullable: false,
            heap: HeapType::Eq,
        }));
    }
    match module.types.get(id.0 as usize) {
        Some(Type::I32 | Type::Boolean | Type::String | Type::Unit) => Ok(StorageType::I32),
        Some(Type::Constructor(TypeConstructor::User(type_id)))
            if newtype_ids.contains(type_id) =>
        {
            let Some(inner) = newtype_field_type(module, *type_id) else {
                return Err(layout_error(span, "newtype must have exactly one field"));
            };
            storage_type(module, inner, aggregate_types, newtype_ids, span)
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
        Some(Type::Variable(_))
        | Some(Type::Constructor(TypeConstructor::Array))
        | Some(Type::Application(_, _))
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

pub(super) fn declaration_shape(
    declaration: &psrs_core::Declaration,
    module: &CoreModule,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
) -> Result<(usize, ValueType), Vec<BackendError>> {
    if !declaration.quantified.is_empty() {
        return Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.name_span,
            "polymorphic declarations are not supported by the first backend slice",
        )]);
    }
    let mut ty = declaration.ty;
    let mut arity = 0;
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
        arity += 1;
        value = body;
    }
    match module.types.get(ty.0 as usize) {
        Some(Type::I32 | Type::String | Type::Unit) => Ok((arity, ValueType::I32)),
        Some(Type::Boolean) => Ok((arity, ValueType::Boolean)),
        Some(Type::Variable(_)) => Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.name_span,
            "polymorphic declarations are not supported by the first backend slice",
        )]),
        Some(Type::Function { .. }) => Err(vec![BackendError::new(
            "P8 closure conversion",
            declaration.span,
            "the first backend slice cannot return a function value",
        )]),
        Some(Type::Constructor(TypeConstructor::User(id))) if enum_types.contains(id) => {
            Ok((arity, ValueType::I32))
        }
        Some(Type::Constructor(TypeConstructor::User(id))) if aggregate_types.contains(id) => {
            Ok((arity, aggregate_value_type()))
        }
        Some(Type::Application(_, _)) => {
            let Some(type_id) = user_type_id(module, ty) else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    declaration.span,
                    "the first backend slice cannot represent aggregate or parameterized types",
                )]);
            };
            if enum_types.contains(&type_id) {
                Ok((arity, ValueType::I32))
            } else if aggregate_types.contains(&type_id) {
                Ok((arity, aggregate_value_type()))
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
            Ok((
                arity,
                scalar_type(
                    module,
                    ty,
                    declaration.span,
                    enum_types,
                    aggregate_types,
                    newtype_ids,
                )?,
            ))
        }
        Some(Type::Constructor(_)) => Err(vec![BackendError::new(
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

pub(super) fn scalar_type(
    module: &CoreModule,
    id: psrs_core::TypeId,
    span: TextRange,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
) -> Result<ValueType, Vec<BackendError>> {
    match module.types.get(id.0 as usize) {
        Some(Type::I32 | Type::String | Type::Unit) => Ok(ValueType::I32),
        Some(Type::Boolean) => Ok(ValueType::Boolean),
        Some(Type::Variable(_)) => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "polymorphic values are not supported by the first backend slice",
        )]),
        Some(Type::Function { .. }) => Err(vec![BackendError::new(
            "P8 closure conversion",
            span,
            "function values are supported only as top-level direct-call targets",
        )]),
        Some(Type::Constructor(TypeConstructor::User(id))) if enum_types.contains(id) => {
            Ok(ValueType::I32)
        }
        Some(Type::Constructor(TypeConstructor::User(id))) if aggregate_types.contains(id) => {
            Ok(aggregate_value_type())
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
            )
        }
        Some(Type::Constructor(_)) => Err(vec![BackendError::new(
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

fn aggregate_value_type() -> ValueType {
    ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Struct,
    })
}

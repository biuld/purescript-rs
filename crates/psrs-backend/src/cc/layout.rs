use super::ValueType;
use crate::BackendError;
use psrs_core::{ExprKind, Module as CoreModule, Type, TypeConstructor};
use psrs_hir::{ExternalKind, ExternalSymbol, TypeId as HirTypeId};
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
pub(super) fn enum_type_ids(module: &CoreModule) -> HashSet<HirTypeId> {
    let mut all_nullary: HashMap<HirTypeId, bool> = HashMap::new();
    for constructor in &module.constructors {
        let entry = all_nullary.entry(constructor.type_id).or_insert(true);
        if constructor.field_count != 0 {
            *entry = false;
        }
    }
    all_nullary
        .into_iter()
        .filter(|(_, nullary)| *nullary)
        .map(|(id, _)| id)
        .collect()
}

pub(super) fn declaration_shape(
    declaration: &psrs_core::Declaration,
    module: &CoreModule,
    enum_types: &HashSet<HirTypeId>,
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
        Some(Type::Constructor(_) | Type::Application(_, _)) => Err(vec![BackendError::new(
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
        Some(Type::Constructor(_) | Type::Application(_, _)) => Err(vec![BackendError::new(
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

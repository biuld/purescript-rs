use super::{Signature, scalar_type};
use crate::BackendError;
use crate::types::{CompositeType, DefinedType};
use psrs_core::{Expr, ExprKind, Module as CoreModule, Type, TypeId};
use psrs_hir::TypeId as HirTypeId;
use std::collections::{HashMap, HashSet};

pub(super) fn append_function_types(
    module: &CoreModule,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, u32>,
    record_types: &HashMap<TypeId, u32>,
    definitions: &mut Vec<DefinedType>,
) -> Result<HashMap<TypeId, u32>, Vec<BackendError>> {
    let needs_function_types = module.declarations.iter().any(|declaration| {
        let mut value = &declaration.value;
        let mut function_parameter = false;
        while let ExprKind::Lambda { binder, body } = &value.kind {
            function_parameter |= is_function_type(module, binder.ty);
            value = body;
        }
        function_parameter || contains_function_value(value, module)
    });
    if !needs_function_types {
        return Ok(HashMap::new());
    }
    let function_type_base = definitions.len() as u32;
    let function_types = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, ty)| {
            matches!(ty, Type::Function { .. }).then_some((
                TypeId(index as u32),
                function_type_base + function_types_count_before(module, index),
            ))
        })
        .collect::<HashMap<_, _>>();
    for (index, ty) in module.types.iter().enumerate() {
        if !matches!(ty, Type::Function { .. }) {
            continue;
        }
        let signature = function_signature(
            module,
            TypeId(index as u32),
            enum_types,
            aggregate_types,
            newtype_ids,
            array_types,
            record_types,
            &function_types,
        )?;
        definitions.push(DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Func {
                parameters: signature.parameters,
                results: vec![signature.result],
            },
        });
    }
    Ok(function_types)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn function_signature(
    module: &CoreModule,
    mut id: TypeId,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, u32>,
    record_types: &HashMap<TypeId, u32>,
    function_types: &HashMap<TypeId, u32>,
) -> Result<Signature, Vec<BackendError>> {
    let mut parameters = Vec::new();
    loop {
        let Some(Type::Function { parameter, result }) = module.types.get(id.0 as usize) else {
            let result = scalar_type(
                module,
                id,
                module.span,
                enum_types,
                aggregate_types,
                newtype_ids,
                array_types,
                record_types,
                function_types,
            )?;
            return Ok(Signature { parameters, result });
        };
        parameters.push(scalar_type(
            module,
            *parameter,
            module.span,
            enum_types,
            aggregate_types,
            newtype_ids,
            array_types,
            record_types,
            function_types,
        )?);
        id = *result;
    }
}

fn function_types_count_before(module: &CoreModule, index: usize) -> u32 {
    module.types[..index]
        .iter()
        .filter(|ty| matches!(ty, Type::Function { .. }))
        .count() as u32
}

fn contains_function_value(expression: &Expr, module: &CoreModule) -> bool {
    match &expression.kind {
        ExprKind::Lambda { .. } => true,
        ExprKind::Let { bindings, body } => {
            bindings.iter().any(|binding| {
                is_function_type(module, binding.binder.ty)
                    || contains_function_value(&binding.value, module)
            }) || contains_function_value(body, module)
        }
        ExprKind::Application(function, argument) => {
            (!matches!(function.kind, ExprKind::Global(_)) && is_function_type(module, function.ty))
                || is_function_type(module, argument.ty)
                || contains_function_value(function, module)
                || contains_function_value(argument, module)
        }
        ExprKind::Constructor { arguments, .. }
        | ExprKind::Array {
            elements: arguments,
        } => arguments
            .iter()
            .any(|argument| contains_function_value(argument, module)),
        ExprKind::Record { fields } => fields
            .iter()
            .any(|(_, value)| contains_function_value(value, module)),
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            contains_function_value(record, module)
        }
        ExprKind::ArrayIndex { array, index } => {
            contains_function_value(array, module) || contains_function_value(index, module)
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            contains_function_value(array, module)
                || contains_function_value(index, module)
                || contains_function_value(value, module)
        }
        ExprKind::Primitive { left, right, .. } => {
            contains_function_value(left, module) || contains_function_value(right, module)
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            contains_function_value(condition, module)
                || contains_function_value(then_branch, module)
                || contains_function_value(else_branch, module)
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            contains_function_value(scrutinee, module)
                || branches
                    .iter()
                    .any(|branch| contains_function_value(&branch.value, module))
        }
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_) => false,
    }
}

fn is_function_type(module: &CoreModule, id: TypeId) -> bool {
    matches!(module.types.get(id.0 as usize), Some(Type::Function { .. }))
}

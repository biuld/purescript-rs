use super::{Signature, scalar_type};
use crate::BackendError;
use crate::cc::{RefShape, ReprId, Representation, RepresentationTable, SignatureId, ValueShape};
use psrs_core::{Expr, ExprKind, Module as CoreModule, Type, TypeId};
use psrs_hir::TypeId as HirTypeId;
use std::collections::{HashMap, HashSet};

pub(super) fn append_function_types(
    module: &CoreModule,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, ReprId>,
    record_types: &HashMap<TypeId, ReprId>,
    representations: &mut RepresentationTable,
) -> Result<FunctionLayouts, Vec<BackendError>> {
    let needs_function_types = module.declarations.iter().any(|declaration| {
        let mut value = &declaration.value;
        let mut result_type = declaration.ty;
        let mut function_parameter = false;
        while let ExprKind::Lambda { binder, body } = &value.kind {
            function_parameter |= is_function_type(module, binder.ty);
            match module.types.get(result_type.0 as usize) {
                Some(Type::Function { result, .. }) => result_type = *result,
                _ => break,
            }
            value = body;
        }
        function_parameter
            || contains_function_value(value, module)
            || is_function_type(module, result_type)
    });
    if !needs_function_types {
        return Ok(FunctionLayouts {
            function_types: HashMap::new(),
        });
    }

    // Constructor schemes and unused library declarations leave function types
    // in the linked table. Laying those out would demand a runtime shape for an
    // ADT the program never references.
    let referenced = referenced_types(module);
    let function_ids = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, ty)| {
            let id = TypeId(index as u32);
            (matches!(ty, Type::Function { .. }) && referenced.contains(&id)).then_some(id)
        })
        .collect::<Vec<_>>();
    let mut provisional_function_types = HashMap::new();
    for id in &function_ids {
        let signature = SignatureId(representations.signatures.len() as u32);
        representations.signatures.push(Signature {
            parameters: Vec::new(),
            result: ValueShape::Integer,
        });
        provisional_function_types.insert(*id, signature);
    }

    let mut function_types = HashMap::new();
    let mut signatures = HashMap::<Signature, SignatureId>::new();
    for id in function_ids {
        let signature = function_signature(
            module,
            id,
            enum_types,
            aggregate_types,
            newtype_ids,
            array_types,
            record_types,
            &provisional_function_types,
        )?;
        let signature_id = if let Some(signature_id) = signatures.get(&signature) {
            *signature_id
        } else {
            let signature_id = provisional_function_types[&id];
            representations.signatures[signature_id.0 as usize] = signature.clone();
            signatures.insert(signature, signature_id);
            signature_id
        };
        function_types.insert(id, signature_id);
    }

    // Nested parameters and results were interned with the provisional id of the
    // function type that produced them. When two structurally identical function
    // types deduplicate, an outer signature can still name the losing type's
    // provisional slot, so resolve every nested closure reference to the final
    // id of the type it came from. `function_types` only ever names a stored
    // slot, so one pass reaches a fixpoint.
    let mut resolved = HashMap::new();
    for (id, provisional) in &provisional_function_types {
        if let Some(final_id) = function_types.get(id) {
            resolved.insert(*provisional, *final_id);
        }
    }
    for signature in &mut representations.signatures {
        remap_nested_signatures(signature, &resolved);
    }

    Ok(FunctionLayouts { function_types })
}

pub(super) struct FunctionLayouts {
    pub(super) function_types: HashMap<TypeId, SignatureId>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn function_signature(
    module: &CoreModule,
    mut id: TypeId,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, ReprId>,
    record_types: &HashMap<TypeId, ReprId>,
    function_types: &HashMap<TypeId, SignatureId>,
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

/// Re-interns the signature table after aggregate normalization has rewritten
/// its representation handles. P8 interns signatures before the canonical
/// aggregate layouts exist, so two signatures whose provisional handles later
/// normalize to the same handle can be distinct `SignatureId`s. This pass
/// deduplicates those and rewrites every nested closure reference and function
/// type to the canonical id, so one `SignatureId` maps to one MIR func type.
pub(super) fn canonicalize_signatures(
    representations: &mut RepresentationTable,
    function_types: &mut HashMap<TypeId, SignatureId>,
) {
    let mut remap = HashMap::<SignatureId, SignatureId>::new();
    let max_passes = representations.signatures.len().saturating_add(2);
    for _ in 0..max_passes {
        let mut canonical = HashMap::<Signature, SignatureId>::new();
        let mut changed = false;
        for index in 0..representations.signatures.len() {
            let id = SignatureId(index as u32);
            let mut signature = representations.signatures[index].clone();
            remap_nested_signatures(&mut signature, &remap);
            let target = resolve_signature(id, &remap);
            let canonical_id = *canonical.entry(signature.clone()).or_insert(target);
            if target != canonical_id {
                remap.insert(id, canonical_id);
                changed = true;
            }
            representations.signatures[index] = signature;
        }
        if !changed {
            break;
        }
    }
    let resolved = remap
        .keys()
        .map(|id| (*id, resolve_signature(*id, &remap)))
        .collect::<HashMap<_, _>>();
    for signature in &mut representations.signatures {
        remap_nested_signatures(signature, &resolved);
    }
    for representation in &mut representations.representations {
        remap_representation_signatures(representation, &resolved);
    }
    for id in function_types.values_mut() {
        if let Some(canonical) = resolved.get(id) {
            *id = *canonical;
        }
    }
}

fn resolve_signature(id: SignatureId, remap: &HashMap<SignatureId, SignatureId>) -> SignatureId {
    let mut current = id;
    while let Some(next) = remap.get(&current).copied() {
        if next == current {
            break;
        }
        current = next;
    }
    current
}

fn remap_nested_signatures(signature: &mut Signature, remap: &HashMap<SignatureId, SignatureId>) {
    for parameter in &mut signature.parameters {
        remap_shape_signature(parameter, remap);
    }
    remap_shape_signature(&mut signature.result, remap);
}

fn remap_shape_signature(shape: &mut ValueShape, remap: &HashMap<SignatureId, SignatureId>) {
    let ValueShape::Reference(reference) = shape else {
        return;
    };
    if let RefShape::Closure(id) = reference.heap
        && let Some(canonical) = remap.get(&id)
    {
        reference.heap = RefShape::Closure(*canonical);
    }
}

fn remap_representation_signatures(
    representation: &mut Representation,
    remap: &HashMap<SignatureId, SignatureId>,
) {
    match representation {
        Representation::Box { value } => remap_shape_signature(value, remap),
        Representation::Product { fields } => {
            for field in fields {
                remap_shape_signature(field, remap);
            }
        }
        Representation::Variant { cases } => {
            for case in cases {
                for field in &mut case.fields {
                    remap_shape_signature(field, remap);
                }
            }
        }
        Representation::Array { element } => remap_shape_signature(element, remap),
    }
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
        ExprKind::RecordUpdate { record, fields } => {
            contains_function_value(record, module)
                || fields
                    .iter()
                    .any(|(_, value)| contains_function_value(value, module))
        }
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
        ExprKind::UnaryPrimitive { value, .. } => contains_function_value(value, module),
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
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => false,
    }
}

fn is_function_type(module: &CoreModule, id: TypeId) -> bool {
    matches!(module.types.get(id.0 as usize), Some(Type::Function { .. }))
}

/// Function types that occur on a remaining declaration, expression, or
/// constructor field. Types left behind by pruned library code are omitted.
fn referenced_types(module: &CoreModule) -> HashSet<TypeId> {
    let mut referenced = HashSet::new();
    let mut visiting = HashSet::new();
    for declaration in &module.declarations {
        record_type(module, declaration.ty, &mut visiting, &mut referenced);
        record_expr(module, &declaration.value, &mut visiting, &mut referenced);
    }
    for constructor in &module.constructors {
        for field in &constructor.field_types {
            record_type(module, *field, &mut visiting, &mut referenced);
        }
    }
    referenced
}

fn record_expr(
    module: &CoreModule,
    expression: &Expr,
    visiting: &mut HashSet<TypeId>,
    referenced: &mut HashSet<TypeId>,
) {
    record_type(module, expression.ty, visiting, referenced);
    match &expression.kind {
        ExprKind::Array { elements } => {
            for element in elements {
                record_expr(module, element, visiting, referenced);
            }
        }
        ExprKind::Record { fields } | ExprKind::RecordUpdate { fields, .. } => {
            if let ExprKind::RecordUpdate { record, .. } = &expression.kind {
                record_expr(module, record, visiting, referenced);
            }
            for (_, value) in fields {
                record_expr(module, value, visiting, referenced);
            }
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            record_expr(module, record, visiting, referenced);
        }
        ExprKind::ArrayIndex { array, index } => {
            record_expr(module, array, visiting, referenced);
            record_expr(module, index, visiting, referenced);
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            record_expr(module, array, visiting, referenced);
            record_expr(module, index, visiting, referenced);
            record_expr(module, value, visiting, referenced);
        }
        ExprKind::Constructor { arguments, .. } => {
            for argument in arguments {
                record_expr(module, argument, visiting, referenced);
            }
        }
        ExprKind::Application(function, argument)
        | ExprKind::Primitive {
            left: function,
            right: argument,
            ..
        } => {
            record_expr(module, function, visiting, referenced);
            record_expr(module, argument, visiting, referenced);
        }
        ExprKind::UnaryPrimitive { value, .. } => {
            record_expr(module, value, visiting, referenced);
        }
        ExprKind::Lambda { binder, body } => {
            record_type(module, binder.ty, visiting, referenced);
            record_expr(module, body, visiting, referenced);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                record_type(module, binding.binder.ty, visiting, referenced);
                record_expr(module, &binding.value, visiting, referenced);
            }
            record_expr(module, body, visiting, referenced);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            record_expr(module, condition, visiting, referenced);
            record_expr(module, then_branch, visiting, referenced);
            record_expr(module, else_branch, visiting, referenced);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            record_expr(module, scrutinee, visiting, referenced);
            for branch in branches {
                record_type(module, branch.pattern.ty, visiting, referenced);
                record_expr(module, &branch.value, visiting, referenced);
            }
        }
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => {}
    }
}

fn record_type(
    module: &CoreModule,
    id: TypeId,
    visiting: &mut HashSet<TypeId>,
    referenced: &mut HashSet<TypeId>,
) {
    if !visiting.insert(id) {
        return;
    }
    match module.types.get(id.0 as usize) {
        Some(Type::Function { parameter, result }) => {
            referenced.insert(id);
            record_type(module, *parameter, visiting, referenced);
            record_type(module, *result, visiting, referenced);
        }
        Some(Type::Application(function, argument)) => {
            record_type(module, *function, visiting, referenced);
            record_type(module, *argument, visiting, referenced);
        }
        Some(Type::Record(fields)) => {
            for (_, field) in fields {
                record_type(module, *field, visiting, referenced);
            }
        }
        Some(Type::OpenRecord { fields, tail }) => {
            record_type(module, *tail, visiting, referenced);
            for (_, field) in fields {
                record_type(module, *field, visiting, referenced);
            }
        }
        _ => {}
    }
}

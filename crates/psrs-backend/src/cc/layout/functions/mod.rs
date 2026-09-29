use super::{Signature, function_arrow_parameters, is_callable_type, scalar_type};
use crate::BackendError;
use crate::cc::{RefShape, ReprId, Representation, RepresentationTable, SignatureId, ValueShape};
use psrs_core::{Module as CoreModule, TypeId};
use psrs_hir::TypeId as HirTypeId;
use std::collections::{HashMap, HashSet};

mod reachable;

use reachable::referenced_types;

pub(super) fn append_function_types(
    module: &CoreModule,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, ReprId>,
    record_types: &HashMap<TypeId, ReprId>,
    representations: &mut RepresentationTable,
) -> Result<FunctionLayouts, Vec<BackendError>> {
    // Constructor schemes and unused library declarations leave callable types
    // in the linked table. Laying those out would demand a runtime shape for an
    // ADT the program never references. Both source arrows and the closure
    // representation of a registered callable constructor are callable, so both
    // receive a signature here.
    let referenced = referenced_types(module);
    let function_ids = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, _ty)| {
            let id = TypeId(index as u32);
            (referenced.contains(&id) && is_callable_type(module, id)).then_some(id)
        })
        .collect::<Vec<_>>();
    if function_ids.is_empty() {
        return Ok(FunctionLayouts {
            function_types: HashMap::new(),
        });
    }

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
    id: TypeId,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    array_types: &HashMap<TypeId, ReprId>,
    record_types: &HashMap<TypeId, ReprId>,
    function_types: &HashMap<TypeId, SignatureId>,
) -> Result<Signature, Vec<BackendError>> {
    let (parameter_ids, result_id) = function_arrow_parameters(module, id);
    let parameters = parameter_ids
        .into_iter()
        .map(|parameter| {
            scalar_type(
                module,
                parameter,
                module.span,
                enum_types,
                aggregate_types,
                newtype_ids,
                array_types,
                record_types,
                function_types,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result = scalar_type(
        module,
        result_id,
        module.span,
        enum_types,
        aggregate_types,
        newtype_ids,
        array_types,
        record_types,
        function_types,
    )?;
    Ok(Signature { parameters, result })
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

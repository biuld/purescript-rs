//! Reachability analysis for target-neutral CC layout requirements.

mod assignments;

use super::layout::LayoutError;
use crate::cc::{
    Module as CcModule, RefShape, Reference, ReprId, Representation, RepresentationTable,
    Signature, SignatureId, ValueShape,
};
use std::collections::{HashMap, HashSet};

use assignments::add_assignments;

pub(super) struct ReachableHandles {
    pub(super) representations: Vec<ReprId>,
    pub(super) signatures: Vec<SignatureId>,
    /// Whether any reachable value, field, element, capture, signature
    /// parameter/result, or constant is a `String`, so the planner reserves the
    /// GC `$string` type.
    pub(super) needs_string: bool,
}

impl ReachableHandles {
    /// Computes reachability for the GC planner, whose closure environments box
    /// integer and number captures for the `eqref` capture array.
    pub(super) fn from_module(module: &CcModule) -> Result<Self, LayoutError> {
        let mut representations = HashSet::new();
        let mut signatures = HashSet::new();
        let mut representation_work = Vec::new();
        let mut signature_work = Vec::new();
        let mut direct_calls = HashSet::new();
        let mut needs_integer_box = false;
        let mut needs_number_box = false;

        for function in &module.functions {
            let value_types = function
                .values
                .iter()
                .map(|value| (value.id, value.ty))
                .collect::<HashMap<_, _>>();
            for value in &function.values {
                add_value(
                    &value.ty,
                    &mut representations,
                    &mut signatures,
                    &mut representation_work,
                    &mut signature_work,
                );
            }
            add_value(
                &function.result_type,
                &mut representations,
                &mut signatures,
                &mut representation_work,
                &mut signature_work,
            );
            add_assignments(
                &function.assignments,
                &mut direct_calls,
                &value_types,
                &mut needs_integer_box,
                &mut needs_number_box,
                &mut representations,
                &mut signatures,
                &mut representation_work,
                &mut signature_work,
            );
        }
        for external in &module.externals {
            if direct_calls.contains(&external.symbol)
                && let Some(signature) = &external.signature
            {
                add_signature_values(
                    signature,
                    &mut representations,
                    &mut signatures,
                    &mut representation_work,
                    &mut signature_work,
                );
                for parameter in &external.payloads.parameters {
                    add_payload_node(parameter, &mut representations, &mut representation_work);
                }
                add_payload_node(
                    &external.payloads.result,
                    &mut representations,
                    &mut representation_work,
                );
            }
        }

        while !representation_work.is_empty() || !signature_work.is_empty() {
            while let Some(id) = representation_work.pop() {
                visit_representation(
                    &module.representations,
                    id,
                    &mut representations,
                    &mut signatures,
                    &mut representation_work,
                    &mut signature_work,
                )?;
            }
            while let Some(id) = signature_work.pop() {
                visit_signature(
                    &module.representations,
                    id,
                    &mut representations,
                    &mut signatures,
                    &mut representation_work,
                    &mut signature_work,
                )?;
            }
        }

        // An erased aggregate field may hold a boxed scalar, so the erased
        // protocol needs the integer and number boxes whenever one is reachable.
        let has_erased = representations.iter().any(|id| {
            module
                .representations
                .representation(*id)
                .is_some_and(representation_has_erased)
        });
        if needs_integer_box || has_erased {
            match module
                .representations
                .representations
                .iter()
                .position(|representation| {
                    matches!(
                        representation,
                        Representation::Box {
                            value: ValueShape::Integer
                        }
                    )
                })
                .map(|index| ReprId(index as u32))
            {
                Some(id) => add_representation(id, &mut representations, &mut representation_work),
                None if needs_integer_box => return Err(LayoutError::MissingIntegerBox),
                None => {}
            }
        }
        if needs_number_box || has_erased {
            match module
                .representations
                .representations
                .iter()
                .position(|representation| {
                    matches!(
                        representation,
                        Representation::Box {
                            value: ValueShape::Number
                        }
                    )
                })
                .map(|index| ReprId(index as u32))
            {
                Some(id) => add_representation(id, &mut representations, &mut representation_work),
                None if needs_number_box => return Err(LayoutError::MissingNumberBox),
                None => {}
            }
        }

        let mut representations = representations.into_iter().collect::<Vec<_>>();
        representations.sort_by_key(|id| id.0);
        let mut signatures = signatures.into_iter().collect::<Vec<_>>();
        signatures.sort_by_key(|id| id.0);
        let needs_string = module.functions.iter().any(|function| {
            function
                .values
                .iter()
                .any(|value| value.ty == ValueShape::String)
                || function.result_type == ValueShape::String
        }) || representations
            .iter()
            .any(|id| representation_has_string(&module.representations, *id))
            || signatures.iter().any(|id| {
                module
                    .representations
                    .signature(*id)
                    .is_some_and(signature_has_string)
            });
        Ok(Self {
            representations,
            signatures,
            needs_string,
        })
    }
}

fn representation_has_string(table: &RepresentationTable, id: ReprId) -> bool {
    let Some(representation) = table.representation(id) else {
        return false;
    };
    let mut visit = |value: &ValueShape| *value == ValueShape::String;
    match representation {
        Representation::Box { value } | Representation::Array { element: value } => visit(value),
        Representation::Product { fields } => fields.iter().any(&mut visit),
        Representation::Variant { cases } => {
            cases.iter().flat_map(|case| &case.fields).any(&mut visit)
        }
    }
}

fn representation_has_erased(representation: &Representation) -> bool {
    let erased = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    });
    let mut visit = |value: &ValueShape| *value == erased;
    match representation {
        Representation::Box { value } | Representation::Array { element: value } => visit(value),
        Representation::Product { fields } => fields.iter().any(&mut visit),
        Representation::Variant { cases } => {
            cases.iter().flat_map(|case| &case.fields).any(&mut visit)
        }
    }
}

fn signature_has_string(signature: &Signature) -> bool {
    signature
        .parameters
        .iter()
        .any(|value| value == &ValueShape::String)
        || signature.result == ValueShape::String
}

fn add_signature_values(
    signature: &Signature,
    representations: &mut HashSet<ReprId>,
    signatures: &mut HashSet<SignatureId>,
    representation_work: &mut Vec<ReprId>,
    signature_work: &mut Vec<SignatureId>,
) {
    for value in &signature.parameters {
        add_value(
            value,
            representations,
            signatures,
            representation_work,
            signature_work,
        );
    }
    add_value(
        &signature.result,
        representations,
        signatures,
        representation_work,
        signature_work,
    );
}

fn add_value(
    value: &ValueShape,
    representations: &mut HashSet<ReprId>,
    signatures: &mut HashSet<SignatureId>,
    representation_work: &mut Vec<ReprId>,
    signature_work: &mut Vec<SignatureId>,
) {
    if let ValueShape::Reference(reference) = value {
        add_reference(
            reference,
            representations,
            signatures,
            representation_work,
            signature_work,
        );
    }
}

pub(super) fn add_reference(
    reference: &Reference,
    representations: &mut HashSet<ReprId>,
    signatures: &mut HashSet<SignatureId>,
    representation_work: &mut Vec<ReprId>,
    signature_work: &mut Vec<SignatureId>,
) {
    match reference.heap {
        RefShape::Repr(id) => add_representation(id, representations, representation_work),
        RefShape::Closure(id) => add_signature(id, signatures, signature_work),
        RefShape::Aggregate | RefShape::Erased => {}
    }
}

/// Adds the representations a payload tree needs, including a nested variant's
/// supertype and any record or array reference it holds.
fn add_payload_node(
    node: &crate::cc::PayloadNode,
    representations: &mut HashSet<ReprId>,
    work: &mut Vec<ReprId>,
) {
    match node {
        crate::cc::PayloadNode::None => {}
        crate::cc::PayloadNode::Value(ValueShape::Reference(Reference {
            heap: RefShape::Repr(repr),
            ..
        })) => add_representation(*repr, representations, work),
        crate::cc::PayloadNode::Value(_) => {}
        crate::cc::PayloadNode::Record {
            representation,
            fields,
        } => {
            add_representation(*representation, representations, work);
            for field in fields {
                add_payload_node(&field.node, representations, work);
            }
        }
        crate::cc::PayloadNode::List {
            representation,
            element,
        } => {
            add_representation(*representation, representations, work);
            add_payload_node(element, representations, work);
        }
        crate::cc::PayloadNode::Variant {
            representation,
            cases,
        } => {
            add_representation(*representation, representations, work);
            for case in cases.iter().flatten() {
                add_payload_node(case, representations, work);
            }
        }
    }
}

pub(super) fn add_representation(
    id: ReprId,
    representations: &mut HashSet<ReprId>,
    work: &mut Vec<ReprId>,
) {
    if representations.insert(id) {
        work.push(id);
    }
}

pub(super) fn add_signature(
    id: SignatureId,
    signatures: &mut HashSet<SignatureId>,
    work: &mut Vec<SignatureId>,
) {
    if signatures.insert(id) {
        work.push(id);
    }
}

fn visit_representation(
    table: &RepresentationTable,
    id: ReprId,
    representations: &mut HashSet<ReprId>,
    signatures: &mut HashSet<SignatureId>,
    representation_work: &mut Vec<ReprId>,
    signature_work: &mut Vec<SignatureId>,
) -> Result<(), LayoutError> {
    let representation = table
        .representation(id)
        .cloned()
        .ok_or(LayoutError::UnknownRepresentation)?;
    let mut visit = |value: &ValueShape| {
        add_value(
            value,
            representations,
            signatures,
            representation_work,
            signature_work,
        );
    };
    match representation {
        Representation::Box { value } | Representation::Array { element: value } => visit(&value),
        Representation::Product { fields } => fields.iter().for_each(&mut visit),
        Representation::Variant { cases } => cases
            .iter()
            .flat_map(|case| &case.fields)
            .for_each(&mut visit),
    }
    Ok(())
}

fn visit_signature(
    table: &RepresentationTable,
    id: SignatureId,
    representations: &mut HashSet<ReprId>,
    signatures: &mut HashSet<SignatureId>,
    representation_work: &mut Vec<ReprId>,
    signature_work: &mut Vec<SignatureId>,
) -> Result<(), LayoutError> {
    let signature = table
        .signature(id)
        .cloned()
        .ok_or(LayoutError::UnknownSignature)?;
    add_signature_values(
        &signature,
        representations,
        signatures,
        representation_work,
        signature_work,
    );
    Ok(())
}

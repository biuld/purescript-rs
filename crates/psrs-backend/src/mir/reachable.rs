//! Reachability analysis for target-neutral CC layout requirements.

use super::layout::LayoutError;
use crate::cc::{
    Assignment, AssignmentKind, Module as CcModule, RefShape, Reference, ReprId, Representation,
    RepresentationTable, Signature, SignatureId, ValueShape,
};
use crate::types::ValueId;
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

pub(super) struct ReachableHandles {
    pub(super) representations: Vec<ReprId>,
    pub(super) signatures: Vec<SignatureId>,
}

impl ReachableHandles {
    pub(super) fn from_module(module: &CcModule) -> Result<Self, LayoutError> {
        Self::from_module_with_gc_boxes(module, true)
    }

    /// Computes reachability for a planner whose closure environments store
    /// numbers directly instead of boxing them for an `eqref` capture array.
    pub(super) fn from_module_without_gc_boxes(module: &CcModule) -> Result<Self, LayoutError> {
        Self::from_module_with_gc_boxes(module, false)
    }

    fn from_module_with_gc_boxes(
        module: &CcModule,
        require_number_box: bool,
    ) -> Result<Self, LayoutError> {
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
        if require_number_box && needs_integer_box {
            let id = module
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
                .ok_or(LayoutError::MissingIntegerBox)?;
            add_representation(id, &mut representations, &mut representation_work);
        }
        if require_number_box && needs_number_box {
            let id = module
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
                .ok_or(LayoutError::MissingNumberBox)?;
            add_representation(id, &mut representations, &mut representation_work);
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

        let mut representations = representations.into_iter().collect::<Vec<_>>();
        representations.sort_by_key(|id| id.0);
        let mut signatures = signatures.into_iter().collect::<Vec<_>>();
        signatures.sort_by_key(|id| id.0);
        Ok(Self {
            representations,
            signatures,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn add_assignments(
    assignments: &[Assignment],
    direct_calls: &mut HashSet<SymbolId>,
    value_types: &HashMap<ValueId, ValueShape>,
    needs_integer_box: &mut bool,
    needs_number_box: &mut bool,
    representations: &mut HashSet<ReprId>,
    signatures: &mut HashSet<SignatureId>,
    representation_work: &mut Vec<ReprId>,
    signature_work: &mut Vec<SignatureId>,
) {
    for assignment in assignments {
        match &assignment.kind {
            AssignmentKind::DirectCall { function, .. } => {
                direct_calls.insert(*function);
            }
            AssignmentKind::FunctionRef {
                signature,
                captures,
                ..
            } => {
                add_signature(*signature, signatures, signature_work);
                *needs_integer_box |= captures
                    .iter()
                    .any(|capture| value_types.get(capture) == Some(&ValueShape::Integer));
                *needs_number_box |= captures
                    .iter()
                    .any(|capture| value_types.get(capture) == Some(&ValueShape::Number));
            }
            AssignmentKind::IndirectCall { signature, .. } => {
                add_signature(*signature, signatures, signature_work);
            }
            AssignmentKind::RepresentationTest { reference, .. }
            | AssignmentKind::RepresentationCast { reference, .. } => add_reference(
                reference,
                representations,
                signatures,
                representation_work,
                signature_work,
            ),
            AssignmentKind::ProductNew { representation, .. }
            | AssignmentKind::ProductGet { representation, .. }
            | AssignmentKind::VariantNew { representation, .. }
            | AssignmentKind::VariantTag { representation, .. }
            | AssignmentKind::VariantGet { representation, .. }
            | AssignmentKind::ArrayNew { representation, .. }
            | AssignmentKind::ArrayGet { representation, .. }
            | AssignmentKind::ArrayClone { representation, .. }
            | AssignmentKind::ArraySet { representation, .. } => {
                add_representation(*representation, representations, representation_work)
            }
            AssignmentKind::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                add_assignments(
                    then_assignments,
                    direct_calls,
                    value_types,
                    needs_integer_box,
                    needs_number_box,
                    representations,
                    signatures,
                    representation_work,
                    signature_work,
                );
                add_assignments(
                    else_assignments,
                    direct_calls,
                    value_types,
                    needs_integer_box,
                    needs_number_box,
                    representations,
                    signatures,
                    representation_work,
                    signature_work,
                );
            }
            AssignmentKind::Constant(_)
            | AssignmentKind::NumberConstant(_)
            | AssignmentKind::StringConstant(_)
            | AssignmentKind::Primitive { .. }
            | AssignmentKind::Unary { .. }
            | AssignmentKind::ArrayLen { .. } => {}
            AssignmentKind::ClosureGetCapture { .. } => {
                *needs_integer_box |=
                    value_types.get(&assignment.destination) == Some(&ValueShape::Integer);
                *needs_number_box |=
                    value_types.get(&assignment.destination) == Some(&ValueShape::Number);
            }
        }
    }
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

fn add_reference(
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

fn add_representation(id: ReprId, representations: &mut HashSet<ReprId>, work: &mut Vec<ReprId>) {
    if representations.insert(id) {
        work.push(id);
    }
}

fn add_signature(
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

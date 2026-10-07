//! Assignment and conversion reachability for the GC planner.

use super::{add_reference, add_representation, add_signature};
use crate::cc::{Assignment, AssignmentKind, ReprId, SignatureId, ValueConversion, ValueShape};
use crate::types::ValueId;
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

#[allow(clippy::too_many_arguments)]
pub(super) fn add_assignments(
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
                *needs_integer_box |= captures.iter().any(|capture| {
                    matches!(
                        value_types.get(capture),
                        Some(&ValueShape::Integer | &ValueShape::String)
                    )
                });
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
            AssignmentKind::AggregateConvert { conversion, .. } => add_conversion(
                &conversion.plan,
                direct_calls,
                representations,
                representation_work,
            ),
            AssignmentKind::ProductNew { representation, .. }
            | AssignmentKind::ProductGet { representation, .. }
            | AssignmentKind::VariantNew { representation, .. }
            | AssignmentKind::VariantTag { representation, .. }
            | AssignmentKind::VariantGet { representation, .. }
            | AssignmentKind::ArrayNew { representation, .. }
            | AssignmentKind::ArrayFill { representation, .. }
            | AssignmentKind::ArrayGet { representation, .. }
            | AssignmentKind::ArrayClone { representation, .. }
            | AssignmentKind::ArraySet { representation, .. }
            | AssignmentKind::ArrayAppend { representation, .. }
            | AssignmentKind::StringToBytes { representation, .. }
            | AssignmentKind::BytesToString { representation, .. } => {
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
            AssignmentKind::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                add_assignments(
                    default_assignments,
                    direct_calls,
                    value_types,
                    needs_integer_box,
                    needs_number_box,
                    representations,
                    signatures,
                    representation_work,
                    signature_work,
                );
                for case in cases {
                    add_assignments(
                        &case.assignments,
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
            }
            AssignmentKind::Constant(_)
            | AssignmentKind::NumberConstant(_)
            | AssignmentKind::StringConstant(_)
            | AssignmentKind::Primitive { .. }
            | AssignmentKind::Unary { .. }
            | AssignmentKind::NumberToString { .. }
            | AssignmentKind::NumberFromDecimal { .. }
            | AssignmentKind::ArrayLen { .. }
            | AssignmentKind::Unreachable => {}
            AssignmentKind::ClosureGetCapture { .. } => {
                *needs_integer_box |= matches!(
                    value_types.get(&assignment.destination),
                    Some(&ValueShape::Integer | &ValueShape::String)
                );
                *needs_number_box |=
                    value_types.get(&assignment.destination) == Some(&ValueShape::Number);
            }
        }
    }
}

pub(super) fn add_conversion(
    conversion: &ValueConversion,
    direct_calls: &mut HashSet<SymbolId>,
    representations: &mut HashSet<ReprId>,
    representation_work: &mut Vec<ReprId>,
) {
    match conversion {
        ValueConversion::FunctionAdapter { function, .. } => {
            direct_calls.insert(*function);
        }
        ValueConversion::Identity => {}
        ValueConversion::BoxScalar { representation, .. }
        | ValueConversion::UnboxScalar { representation, .. } => {
            add_representation(*representation, representations, representation_work)
        }
        ValueConversion::EraseReference => {}
        ValueConversion::RecoverReference { evidence, .. } => {
            if let crate::cc::RecoveryEvidence::ErasedVariantField { variant, .. } = evidence {
                add_representation(*variant, representations, representation_work);
            }
        }
        ValueConversion::Sequence(steps) => {
            for step in steps {
                add_conversion(step, direct_calls, representations, representation_work);
            }
        }
        ValueConversion::ArrayMap {
            source,
            target,
            element,
        } => {
            add_representation(*source, representations, representation_work);
            add_representation(*target, representations, representation_work);
            add_conversion(element, direct_calls, representations, representation_work);
        }
        ValueConversion::ProductMap {
            source,
            target,
            fields,
            ..
        } => {
            add_representation(*source, representations, representation_work);
            add_representation(*target, representations, representation_work);
            for field in fields {
                add_conversion(field, direct_calls, representations, representation_work);
            }
        }
    }
}

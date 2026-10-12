use super::super::helpers::{assignment_error, declared_shape};
use crate::BackendError;
use crate::cc::Signature;
use crate::cc::control_contract::ControlImplementation;
use crate::cc::{Assignment, AssignmentKind, RepresentationTable, ValueId, ValueShape};
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(super) fn verify(
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
    table: &RepresentationTable,
    assignments: &[Assignment],
    signatures: &HashMap<SymbolId, Signature>,
) -> Result<Vec<ValueId>, Vec<BackendError>> {
    let AssignmentKind::ControlEvidence {
        value,
        implementation,
        arguments,
        contract,
    } = &assignment.kind
    else {
        unreachable!()
    };
    let proof = table
        .control_contracts
        .get(contract.0 as usize)
        .ok_or_else(|| {
            assignment_error(
                assignment,
                "control evidence has no checked source contract",
            )
        })?;
    if proof.arguments().len() != arguments.len() {
        return Err(assignment_error(
            assignment,
            "control evidence argument count disagrees with its checked source contract",
        ));
    }
    if declared_shape(declared, *value, assignment)?
        != declared_shape(declared, assignment.destination, assignment)?
    {
        return Err(assignment_error(
            assignment,
            "control evidence changes its runtime result shape",
        ));
    }
    let carrier_shape = match proof.implementation {
        ControlImplementation::Frame { representation, .. } => {
            Some(super::super::helpers::repr_shape(representation))
        }
        ControlImplementation::Call {
            signature,
            role: crate::cc::ControlHelperRole::Once,
        } => table.signature(signature).map(|signature| signature.result),
        _ => None,
    };
    if carrier_shape.is_some_and(|shape| declared.get(value).copied() != Some(shape)) {
        return Err(assignment_error(
            assignment,
            "control carrier result must retain its nominal representation",
        ));
    }
    let definition = assignments
        .iter()
        .find(|candidate| candidate.destination == *implementation)
        .ok_or_else(|| {
            assignment_error(assignment, "control implementation has no local definition")
        })?;
    if matches!(proof.implementation, ControlImplementation::Frame { .. })
        && value != implementation
    {
        return Err(assignment_error(
            assignment,
            "control evidence result is not its checked frame",
        ));
    }
    let operands = match (&proof.implementation, &definition.kind) {
        (
            ControlImplementation::Frame {
                representation,
                case,
            },
            AssignmentKind::VariantNew {
                representation: actual,
                case: tag,
                fields,
                ..
            },
        ) if representation == actual && case == tag => fields,
        (
            ControlImplementation::Call { signature, role },
            AssignmentKind::DirectCall {
                function,
                arguments,
                contract: Some(contract),
            },
        ) if signatures.get(function) == table.signature(*signature)
            && table
                .control_helpers
                .get(contract.0 as usize)
                .is_some_and(|helper| helper.role() == *role) =>
        {
            arguments
        }
        _ => {
            return Err(assignment_error(
                assignment,
                "control implementation disagrees with its checked frame or helper plan",
            ));
        }
    };
    if operands.len() < arguments.len()
        || !arguments
            .iter()
            .zip(operands)
            .all(|(source, actual)| origin(*source, assignments) == origin(*actual, assignments))
    {
        return Err(assignment_error(
            assignment,
            "control implementation does not preserve its checked source operands",
        ));
    }
    for (argument, expected) in arguments.iter().zip(proof.argument_identities()) {
        let origin = origin(*argument, assignments);
        if let Some(Assignment {
            kind: AssignmentKind::ControlEvidence { contract, .. },
            ..
        }) = assignments
            .iter()
            .find(|candidate| candidate.destination == origin)
        {
            let producer = table
                .control_contracts
                .get(contract.0 as usize)
                .ok_or_else(|| {
                    assignment_error(assignment, "control operand has no checked source contract")
                })?;
            if producer.result_identity() != *expected {
                return Err(assignment_error(
                    assignment,
                    "control operand source type disagrees with its checked domain",
                ));
            }
        }
    }
    let mut uses = arguments.clone();
    uses.extend([*value, *implementation]);
    Ok(uses)
}

fn origin(value: ValueId, assignments: &[Assignment]) -> ValueId {
    crate::cc::value_identity::reference_origin(value, |value| {
        assignments
            .iter()
            .find(|assignment| assignment.destination == value)
            .map(|assignment| &assignment.kind)
    })
}

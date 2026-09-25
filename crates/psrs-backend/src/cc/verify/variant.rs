use super::helpers::{
    aggregate_shape, assignment_error, declared_shape, repr_shape, require_destination,
    verify_embedded_destination,
};
use crate::BackendError;
use crate::cc::{
    Assignment, AssignmentKind, Representation, RepresentationTable, ValueId, ValueShape,
};
use std::collections::HashMap;

pub(super) fn verify_variant_assignment(
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
    table: &RepresentationTable,
) -> Result<Vec<ValueId>, Vec<BackendError>> {
    match &assignment.kind {
        AssignmentKind::VariantNew {
            destination,
            representation,
            case,
            fields,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            let cases = variant_cases(table, *representation, assignment)?;
            let Some(selected) = cases.iter().find(|candidate| candidate.tag == *case) else {
                return Err(assignment_error(assignment, "unknown variant case tag"));
            };
            if fields.len() != selected.fields.len()
                || fields
                    .iter()
                    .zip(&selected.fields)
                    .any(|(value, shape)| declared.get(value).copied() != Some(*shape))
            {
                return Err(assignment_error(
                    assignment,
                    "variant fields have incompatible shapes",
                ));
            }
            let destination_shape = declared_shape(declared, *destination, assignment)?;
            if destination_shape != aggregate_shape()
                && destination_shape != repr_shape(*representation)
            {
                return Err(assignment_error(
                    assignment,
                    "variant construction has an incompatible result shape",
                ));
            }
            Ok(fields.clone())
        }
        AssignmentKind::VariantTag {
            destination,
            representation,
            value,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            variant_cases(table, *representation, assignment)?;
            verify_variant_source(declared, *value, *representation, assignment)?;
            require_destination(
                declared,
                assignment,
                ValueShape::Integer,
                "variant tag must produce Integer",
            )?;
            Ok(vec![*value])
        }
        AssignmentKind::VariantGet {
            destination,
            representation,
            case,
            field,
            value,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            let cases = variant_cases(table, *representation, assignment)?;
            let Some(shape) = cases
                .iter()
                .find(|candidate| candidate.tag == *case)
                .and_then(|selected| selected.fields.get(*field as usize))
            else {
                return Err(assignment_error(
                    assignment,
                    "variant case or field is out of range",
                ));
            };
            verify_variant_source(declared, *value, *representation, assignment)?;
            require_destination(
                declared,
                assignment,
                *shape,
                "variant field has an incompatible result shape",
            )?;
            Ok(vec![*value])
        }
        _ => unreachable!("variant verifier received another operation"),
    }
}

fn variant_cases<'a>(
    table: &'a RepresentationTable,
    id: crate::cc::ReprId,
    assignment: &Assignment,
) -> Result<&'a [crate::cc::VariantCase], Vec<BackendError>> {
    match table.representation(id) {
        Some(Representation::Variant { cases }) => Ok(cases),
        _ => Err(assignment_error(
            assignment,
            "variant operation requires a variant representation",
        )),
    }
}

fn verify_variant_source(
    declared: &HashMap<ValueId, ValueShape>,
    value: ValueId,
    representation: crate::cc::ReprId,
    assignment: &Assignment,
) -> Result<(), Vec<BackendError>> {
    let shape = declared_shape(declared, value, assignment)?;
    if shape != aggregate_shape() && shape != repr_shape(representation) {
        return Err(assignment_error(
            assignment,
            "variant source has an incompatible shape",
        ));
    }
    Ok(())
}

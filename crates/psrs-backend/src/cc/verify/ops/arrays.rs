//! CC verification of the array operations.
//!
//! Each rule pairs the named array representation with the value it reads or
//! writes, so an `array.new`, `array.len`, `array.get`, `array.clone`, or
//! `array.set` can only name an array that actually holds those elements.

use super::super::helpers::*;
use crate::BackendError;
use crate::cc::Representation;
use crate::cc::{Assignment, AssignmentKind, RepresentationTable, ValueId, ValueShape};
use std::collections::HashMap;

/// Verifies one array assignment and records the values it reads.
pub(super) fn verify_array_assignment(
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
    table: &RepresentationTable,
    uses: &mut Vec<ValueId>,
) -> Result<(), Vec<BackendError>> {
    match &assignment.kind {
        AssignmentKind::ArrayNew {
            destination,
            representation,
            elements,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            let Some(Representation::Array { element }) = table.representation(*representation)
            else {
                return Err(assignment_error(
                    assignment,
                    "array construction requires an array representation",
                ));
            };
            if elements
                .iter()
                .any(|element_id| declared.get(element_id).copied() != Some(*element))
            {
                return Err(assignment_error(
                    assignment,
                    "array construction elements have incompatible shapes",
                ));
            }
            require_destination(
                declared,
                assignment,
                repr_shape(*representation),
                "array construction has an incompatible result shape",
            )?;
            uses.extend(elements.iter().copied());
        }
        AssignmentKind::ArrayFill {
            destination,
            representation,
            length,
            value,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            let element = verify_array_representation(table, *representation, assignment)?;
            require_value_shape(declared, *length, ValueShape::Integer, assignment)?;
            require_value_shape(declared, *value, element, assignment)?;
            require_destination(
                declared,
                assignment,
                repr_shape(*representation),
                "arrayFill has an incompatible result shape",
            )?;
            uses.extend([*length, *value]);
        }
        AssignmentKind::ArrayLen { destination, value } => {
            verify_embedded_destination(assignment, *destination)?;
            verify_array_value(declared, *value, table, None, assignment)?;
            require_destination(
                declared,
                assignment,
                ValueShape::Integer,
                "array length must produce Integer",
            )?;
            uses.push(*value);
        }
        AssignmentKind::ArrayGet {
            destination,
            representation,
            value,
            index,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            let element = verify_array_representation(table, *representation, assignment)?;
            verify_array_value(declared, *value, table, Some(*representation), assignment)?;
            require_value_shape(declared, *index, ValueShape::Integer, assignment)?;
            require_destination(
                declared,
                assignment,
                element,
                "array projection has an incompatible result shape",
            )?;
            uses.extend([*value, *index]);
        }
        AssignmentKind::ArrayClone {
            destination,
            representation,
            value,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            verify_array_representation(table, *representation, assignment)?;
            verify_array_value(declared, *value, table, Some(*representation), assignment)?;
            require_destination(
                declared,
                assignment,
                repr_shape(*representation),
                "array clone has an incompatible result shape",
            )?;
            uses.push(*value);
        }
        AssignmentKind::ArraySet {
            destination,
            representation,
            value,
            index,
            new_value,
        } => {
            if assignment.destination != *destination || assignment.destination != *value {
                return Err(assignment_error(
                    assignment,
                    "array update destination must be the updated array",
                ));
            }
            let element = verify_array_representation(table, *representation, assignment)?;
            verify_array_value(declared, *value, table, Some(*representation), assignment)?;
            require_value_shape(declared, *index, ValueShape::Integer, assignment)?;
            require_value_shape(declared, *new_value, element, assignment)?;
            uses.extend([*value, *index, *new_value]);
        }
        AssignmentKind::ArrayAppend {
            destination,
            representation,
            left,
            right,
        } => {
            verify_embedded_destination(assignment, *destination)?;
            verify_array_representation(table, *representation, assignment)?;
            verify_array_value(declared, *left, table, Some(*representation), assignment)?;
            verify_array_value(declared, *right, table, Some(*representation), assignment)?;
            require_destination(
                declared,
                assignment,
                repr_shape(*representation),
                "array append has an incompatible result shape",
            )?;
            uses.extend([*left, *right]);
        }
        _ => unreachable!("array verifier received another assignment"),
    }
    Ok(())
}

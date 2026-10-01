//! CC verification of the `String`/`Array Int` byte conversions.
//!
//! `stringToBytes` reads a `String` and produces the `Array Int`
//! representation; `bytesToString` is the reverse. Neither admits any other
//! element type: the byte form is exactly `Int`, one element per UTF-8 byte.

use super::super::helpers::{
    repr_shape, require_destination, require_value_shape, verify_array_value,
    verify_byte_array_representation, verify_embedded_destination,
};
use crate::BackendError;
use crate::cc::{Assignment, ReprId, RepresentationTable, ValueId, ValueShape};
use std::collections::HashMap;

/// Verifies one conversion assignment. `from_string` selects the direction:
/// `stringToBytes` reads a `String`, `bytesToString` builds one.
pub(super) fn verify_conversion(
    assignment: &Assignment,
    representation: ReprId,
    value: ValueId,
    declared: &HashMap<ValueId, ValueShape>,
    table: &RepresentationTable,
    uses: &mut Vec<ValueId>,
    from_string: bool,
) -> Result<(), Vec<BackendError>> {
    verify_embedded_destination(assignment, assignment.destination)?;
    verify_byte_array_representation(table, representation, assignment)?;
    if from_string {
        require_value_shape(declared, value, ValueShape::String, assignment)?;
        require_destination(
            declared,
            assignment,
            repr_shape(representation),
            "stringToBytes has an incompatible result shape",
        )?;
    } else {
        verify_array_value(declared, value, table, Some(representation), assignment)?;
        require_destination(
            declared,
            assignment,
            ValueShape::String,
            "bytesToString has an incompatible result shape",
        )?;
    }
    uses.push(value);
    Ok(())
}

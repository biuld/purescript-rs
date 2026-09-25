use super::helpers::{assignment_error, declared_shape};
use crate::BackendError;
use crate::cc::{Assignment, RefShape, Reference, ValueId, ValueShape};
use std::collections::HashMap;

/// Checks that a `RepresentationCast`/`RepresentationTest` only crosses a heap
/// boundary through the exact non-null erased requirement.
///
/// The design requires every erased value to have the shape
/// `Reference { nullable: false, heap: Erased }`. A cast that claims a nullable
/// erased endpoint is therefore rejected: a concrete reference may only be
/// erased to, or recovered from, the non-null erased shape. A same-heap change
/// of nullability is allowed because P8 uses it to carry a reference across an
/// aggregate loop without changing its representation.
pub(super) fn verify_erased_adaptation(
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
    value: ValueId,
    target: &Reference,
) -> Result<(), Vec<BackendError>> {
    let source = declared_shape(declared, value, assignment)?;
    let ValueShape::Reference(source) = source else {
        return Err(assignment_error(
            assignment,
            "representation adaptation requires a reference operand",
        ));
    };
    if source.heap == target.heap {
        return Ok(());
    }
    if is_erased(&source) || is_erased(target) {
        return Ok(());
    }
    Err(assignment_error(
        assignment,
        "representation cast must preserve a reference heap or adapt an erased value",
    ))
}

fn is_erased(reference: &Reference) -> bool {
    reference
        == &Reference {
            nullable: false,
            heap: RefShape::Erased,
        }
}

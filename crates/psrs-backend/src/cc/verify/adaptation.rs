use super::helpers::{assignment_error, declared_shape};
use crate::BackendError;
use crate::cc::{Assignment, RefShape, Reference, ValueId, ValueShape};
use std::collections::HashMap;

pub(super) fn verify_erased_adaptation(
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
    value: ValueId,
    target: &Reference,
) -> Result<(), Vec<BackendError>> {
    let source = declared_shape(declared, value, assignment)?;
    let source_erased = matches!(
        source,
        ValueShape::Reference(Reference {
            heap: RefShape::Erased,
            ..
        })
    );
    if !source_erased && target.heap != RefShape::Erased {
        return Err(assignment_error(
            assignment,
            "representation test or cast is reserved for erased-value adaptation",
        ));
    }
    Ok(())
}

use super::*;

pub(super) fn verify_get(
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
    table: &RepresentationTable,
) -> Result<Vec<ValueId>, Vec<BackendError>> {
    let AssignmentKind::ProductGet {
        destination,
        representation,
        field,
        value,
    } = &assignment.kind
    else {
        unreachable!("product projection verifier dispatch");
    };
    verify_embedded_destination(assignment, *destination)?;
    let representation_id = *representation;
    let Some(representation) = table.representation(representation_id) else {
        return Err(assignment_error(
            assignment,
            "unknown representation handle",
        ));
    };
    let (expected, source, boxed) = match representation {
        Representation::Box { value: expected } if *field == 0 => {
            (*expected, Some(repr_shape(representation_id)), true)
        }
        Representation::Product { fields } => {
            let Some(expected) = fields.get(*field as usize).copied() else {
                return Err(assignment_error(
                    assignment,
                    "product projection field is out of range",
                ));
            };
            (expected, None, false)
        }
        Representation::Box { .. } => {
            return Err(assignment_error(
                assignment,
                "box projection field is out of range",
            ));
        }
        _ => {
            return Err(assignment_error(
                assignment,
                "product projection requires a product or box representation",
            ));
        }
    };
    if let Some(source) = source {
        require_value_shape(declared, *value, source, assignment)?;
    } else {
        verify_product_value(declared, *value, representation_id, assignment)?;
    }
    // The shared i32 box stores an `Integer` field but is also the
    // erased home of `Boolean`. Their checks are performed
    // explicitly by the boxing/unboxing path, so a projection may
    // declare either i32-family shape.
    let destination_shape = declared.get(&assignment.destination).copied();
    let result_ok = match (boxed, expected) {
        (true, ValueShape::Integer) => matches!(
            destination_shape,
            Some(ValueShape::Integer | ValueShape::Boolean)
        ),
        _ => destination_shape == Some(expected),
    };
    if !result_ok {
        return Err(assignment_error(
            assignment,
            "product projection has an incompatible result shape",
        ));
    }
    Ok(vec![*value])
}

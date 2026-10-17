use super::*;

pub(super) fn verify(
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
    table: &RepresentationTable,
) -> Result<Vec<ValueId>, Vec<BackendError>> {
    let AssignmentKind::Record {
        destination,
        record,
        operation,
    } = &assignment.kind
    else {
        unreachable!("record verifier dispatch");
    };
    verify_embedded_destination(assignment, *destination)?;
    let storage = table.record_storage().ok_or_else(|| {
        assignment_error(
            assignment,
            "record operation has no registered storage schema",
        )
    })?;
    require_value_shape(
        declared,
        *record,
        repr_shape(storage.representation()),
        assignment,
    )?;
    let mut uses = vec![*record];
    for (value, shape) in operation.arguments() {
        require_value_shape(declared, value, shape, assignment)?;
        uses.push(value);
    }
    require_value_shape(
        declared,
        *destination,
        operation.result_shape(storage),
        assignment,
    )?;
    Ok(uses)
}

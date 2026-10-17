use super::*;
use crate::cc::RecordOperation;

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
    if matches!(operation, RecordOperation::Build { .. }) {
        return Err(assignment_error(
            assignment,
            "record build must retain its checked provider binding",
        ));
    }
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
    for (value, shape) in operation.arguments(storage) {
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

pub(super) fn verify_primitive(
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
    table: &RepresentationTable,
) -> Result<Vec<ValueId>, Vec<BackendError>> {
    let AssignmentKind::RecordPrimitive {
        record, operation, ..
    } = &assignment.kind
    else {
        unreachable!("record primitive verifier dispatch");
    };
    verify_embedded_destination(assignment, assignment.destination)?;
    let storage = table.record_storage().ok_or_else(|| {
        assignment_error(
            assignment,
            "record primitive has no registered storage schema",
        )
    })?;
    let build = matches!(operation, RecordOperation::Build { .. });
    if build != record.is_none() {
        return Err(assignment_error(
            assignment,
            "record primitive operation has an invalid record operand",
        ));
    }
    let mut uses = Vec::new();
    if let Some(record) = record {
        require_value_shape(
            declared,
            *record,
            repr_shape(storage.representation()),
            assignment,
        )?;
        uses.push(*record);
    }
    for (value, shape) in operation.arguments(storage) {
        require_value_shape(declared, value, shape, assignment)?;
        uses.push(value);
    }
    require_value_shape(
        declared,
        assignment.destination,
        operation.result_shape(storage),
        assignment,
    )?;
    Ok(uses)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_build_without_checked_provider_evidence() {
        let destination = ValueId(3);
        let assignment = Assignment {
            destination,
            kind: AssignmentKind::Record {
                destination,
                record: ValueId(0),
                operation: RecordOperation::Build {
                    labels: ValueId(1),
                    values: ValueId(2),
                },
            },
            span: psrs_span::TextRange::new(0, 1),
        };
        let errors = verify(
            &assignment,
            &HashMap::new(),
            &RepresentationTable::default(),
        )
        .expect_err("record build must retain its checked provider identity");
        assert!(errors.iter().any(|error| {
            error
                .message
                .contains("record build must retain its checked provider binding")
        }));
    }
}

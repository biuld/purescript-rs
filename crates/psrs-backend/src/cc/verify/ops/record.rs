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

    #[test]
    fn every_record_read_and_update_rejects_forged_operands_at_the_public_boundary() {
        use crate::BackendErrorKind;
        use crate::cc::{Function, Module, ValueDecl};
        use psrs_hir::ModuleId;

        let mut table = RepresentationTable::default();
        let storage = table.ensure_record_storage();
        let header = repr_shape(storage.representation());
        let erased = crate::cc::payload::erased_shape();
        // These are the contract's independently specified operands/results,
        // not obtained from RecordOperation::arguments or result_shape.
        let cases = [
            (RecordOperation::Length, ValueShape::Integer, vec![]),
            (
                RecordOperation::LabelAt { index: ValueId(1) },
                ValueShape::String,
                vec![1],
            ),
            (
                RecordOperation::Find { label: ValueId(2) },
                ValueShape::Integer,
                vec![2],
            ),
            (RecordOperation::Read { index: ValueId(1) }, erased, vec![1]),
            (
                RecordOperation::Update {
                    replacements: vec![(ValueId(2), ValueId(3))],
                },
                header,
                vec![2, 3],
            ),
        ];
        for (operation, result_type, operands) in cases {
            let span = TextRange::new(10, 20);
            let baseline = Module {
                name: "RecordOperandContract".into(),
                externals: Vec::new(),
                representations: table.clone(),
                entry: None,
                span,
                functions: vec![Function {
                    symbol: SymbolId::new(ModuleId(0), 0),
                    name: "operation".into(),
                    parameters: (0..4).map(ValueId).collect(),
                    values: [
                        header,
                        ValueShape::Integer,
                        ValueShape::String,
                        erased,
                        result_type,
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(id, ty)| ValueDecl {
                        id: ValueId(id as u32),
                        ty,
                    })
                    .collect(),
                    assignments: vec![Assignment {
                        destination: ValueId(4),
                        span,
                        kind: AssignmentKind::Record {
                            destination: ValueId(4),
                            record: ValueId(0),
                            operation: operation.clone(),
                        },
                    }],
                    result: ValueId(4),
                    result_type,
                    span,
                }],
            };
            crate::cc::verify_module(&baseline)
                .expect("the independently typed operation is valid");
            let reject = |module: &Module, mutation: &str| {
                let errors = crate::cc::verify_module(module).expect_err(mutation);
                assert!(
                    errors
                        .iter()
                        .all(|error| error.kind == BackendErrorKind::InvalidCompilerIr),
                    "{mutation}: {errors:?}"
                );
                assert!(
                    errors.iter().any(|error| error.span == span),
                    "{mutation}: {errors:?}"
                );
            };
            for operand in std::iter::once(0).chain(operands) {
                let mut wrong_shape = baseline.clone();
                wrong_shape.functions[0].values[operand].ty = ValueShape::Number;
                reject(
                    &wrong_shape,
                    "a record operand cannot change its checked use type",
                );
                let mut missing = baseline.clone();
                missing.functions[0].values.remove(operand);
                reject(&missing, "a record operand must have a declaration");
            }
            let mut wrong_result = baseline.clone();
            wrong_result.functions[0].values[4].ty = ValueShape::Number;
            wrong_result.functions[0].result_type = ValueShape::Number;
            reject(
                &wrong_result,
                "the result must match the operation's contract",
            );
            let mut wrong_destination = baseline.clone();
            let AssignmentKind::Record { destination, .. } =
                &mut wrong_destination.functions[0].assignments[0].kind
            else {
                unreachable!()
            };
            *destination = ValueId(1);
            reject(
                &wrong_destination,
                "embedded and published destinations must agree",
            );
        }
    }
}

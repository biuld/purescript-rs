//! The checked source-to-CC operation projection for record provider exports.

use crate::cc::RecordOperation;
use crate::cc::ValueId;
use psrs_runtime::{RecordPrimitive, RecordProjection};

pub(crate) fn operation_for_call(
    primitive: RecordPrimitive,
    arguments: &[ValueId],
) -> Option<(Option<ValueId>, RecordOperation)> {
    let contract = primitive.source_contract();
    if arguments.len() != contract.parameters.len() {
        return None;
    }
    let argument = |index: usize| arguments.get(index).copied();
    Some(match contract.projection {
        RecordProjection::Length { record } => (Some(argument(record)?), RecordOperation::Length),
        RecordProjection::KeyAt { index, record } => (
            Some(argument(record)?),
            RecordOperation::LabelAt {
                index: argument(index)?,
            },
        ),
        RecordProjection::PayloadAt { index, record } => (
            Some(argument(record)?),
            RecordOperation::Read {
                index: argument(index)?,
            },
        ),
        RecordProjection::Build { labels, values } => (
            None,
            RecordOperation::Build {
                labels: argument(labels)?,
                values: argument(values)?,
            },
        ),
    })
}

pub(crate) fn operation_matches(
    primitive: RecordPrimitive,
    record: Option<ValueId>,
    operation: &RecordOperation,
) -> bool {
    let contract = primitive.source_contract();
    let mut arguments = vec![None; contract.parameters.len()];
    let set = |arguments: &mut [Option<ValueId>], index: usize, value: ValueId| {
        let Some(slot) = arguments.get_mut(index) else {
            return false;
        };
        *slot = Some(value);
        true
    };
    let valid = match (contract.projection, record, operation) {
        (RecordProjection::Length { record: position }, Some(record), RecordOperation::Length) => {
            set(&mut arguments, position, record)
        }
        (
            RecordProjection::KeyAt {
                index,
                record: position,
            },
            Some(record),
            RecordOperation::LabelAt { index: value },
        ) => set(&mut arguments, index, *value) && set(&mut arguments, position, record),
        (
            RecordProjection::PayloadAt {
                index,
                record: position,
            },
            Some(record),
            RecordOperation::Read { index: value },
        ) => set(&mut arguments, index, *value) && set(&mut arguments, position, record),
        (
            RecordProjection::Build { labels, values },
            None,
            RecordOperation::Build {
                labels: label_value,
                values: payload_value,
            },
        ) => {
            set(&mut arguments, labels, *label_value) && set(&mut arguments, values, *payload_value)
        }
        _ => false,
    };
    valid
        && arguments.iter().all(Option::is_some)
        && operation_for_call(
            primitive,
            &arguments.into_iter().flatten().collect::<Vec<_>>(),
        ) == Some((record, operation.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_descriptors_project_and_recover_each_shared_record_operation() {
        let record = ValueId(9);
        let index = ValueId(2);
        let labels = ValueId(3);
        let values = ValueId(4);
        let cases = [
            (
                RecordPrimitive::Length,
                vec![record],
                Some(record),
                RecordOperation::Length,
            ),
            (
                RecordPrimitive::KeyAt,
                vec![index, record],
                Some(record),
                RecordOperation::LabelAt { index },
            ),
            (
                RecordPrimitive::PayloadAt,
                vec![index, record],
                Some(record),
                RecordOperation::Read { index },
            ),
            (
                RecordPrimitive::Build,
                vec![labels, values],
                None,
                RecordOperation::Build { labels, values },
            ),
        ];
        for (primitive, arguments, record, operation) in cases {
            assert_eq!(
                operation_for_call(primitive, &arguments),
                Some((record, operation.clone()))
            );
            assert!(operation_matches(primitive, record, &operation));
        }

        assert!(!operation_matches(
            RecordPrimitive::Build,
            Some(record),
            &RecordOperation::Build { labels, values },
        ));
        assert!(!operation_matches(
            RecordPrimitive::Build,
            None,
            &RecordOperation::Length,
        ));
        assert!(!operation_matches(
            RecordPrimitive::PayloadAt,
            Some(record),
            &RecordOperation::LabelAt { index },
        ));
    }
}

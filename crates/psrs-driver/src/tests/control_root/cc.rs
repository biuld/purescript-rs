use super::{Mutation, R, SOURCE, binding};
use psrs_backend::cc::{Assignment, AssignmentKind as A, ValueId, ValueShape};
use psrs_hir::Intrinsic;

pub(super) fn reject(mutation: Mutation) {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (start, start_id) = binding(&input.cc.representations, R::Start);
    let (check, check_id) = binding(&input.cc.representations, R::Check);
    let close = binding(&input.cc.representations, R::Close).1;
    let table = &input.cc.representations;
    let function = input.cc.functions.iter_mut().find(|function| {
        if matches!(mutation, Mutation::StartStatus) { return function.symbol == start; }
        function.assignments.iter().any(|assignment| matches!(&assignment.kind,
            A::ControlEvidence { contract, .. } if table.control_contracts[contract.0 as usize].operation() == Intrinsic::ControlRun))
    }).unwrap();
    if matches!(mutation, Mutation::StartStatus) {
        let A::ArrayNew { elements, .. } = &mut function
            .assignments
            .iter_mut()
            .find(|assignment| matches!(assignment.kind, A::ArrayNew { .. }))
            .unwrap()
            .kind
        else {
            unreachable!()
        };
        elements.clear();
    } else {
        let call_value = function
            .assignments
            .iter()
            .find_map(|assignment| match &assignment.kind {
                A::ControlEvidence {
                    contract,
                    implementation,
                    ..
                } if table.control_contracts[contract.0 as usize].operation()
                    == Intrinsic::ControlRun =>
                {
                    Some(*implementation)
                }
                _ => None,
            })
            .unwrap();
        let call_index = function
            .assignments
            .iter()
            .position(|assignment| assignment.destination == call_value)
            .unwrap();
        let A::DirectCall { arguments, .. } = &function.assignments[call_index].kind else {
            unreachable!()
        };
        let extent = arguments[4];
        let context = arguments[5];
        let empty = arguments[3];
        let extra = ValueId(
            function
                .values
                .iter()
                .map(|value| value.id.0)
                .max()
                .unwrap()
                + 1,
        );
        let extent_ty = function
            .values
            .iter()
            .find(|value| value.id == extent)
            .unwrap()
            .ty;
        let start_index = function
            .assignments
            .iter()
            .position(|assignment| assignment.destination == extent)
            .unwrap();
        if matches!(
            mutation,
            Mutation::DispatchExtent | Mutation::ExecutionExtent | Mutation::WrongClose
        ) {
            function.values.push(psrs_backend::cc::ValueDecl {
                id: extra,
                ty: extent_ty,
            });
            function.assignments.insert(
                start_index + 1,
                Assignment {
                    destination: extra,
                    kind: A::DirectCall {
                        contract: Some(start_id),
                        function: start,
                        arguments: vec![],
                    },
                    span: function.span,
                },
            );
        }
        match mutation {
            Mutation::DispatchExtent => {
                let A::VariantNew { fields, .. } = &mut function
                    .assignments
                    .iter_mut()
                    .find(|assignment| assignment.destination == context)
                    .unwrap()
                    .kind
                else {
                    unreachable!()
                };
                fields[0] = extra;
            }
            Mutation::ExecutionExtent => {
                let A::DirectCall { arguments, .. } = &mut function
                    .assignments
                    .iter_mut()
                    .find(|assignment| assignment.destination == call_value)
                    .unwrap()
                    .kind
                else {
                    unreachable!()
                };
                arguments[4] = extra;
            }
            Mutation::Queue => {
                let A::VariantNew { representation, .. } = function
                    .assignments
                    .iter()
                    .find(|assignment| assignment.destination == empty)
                    .unwrap()
                    .kind
                else {
                    unreachable!()
                };
                let ty = function
                    .values
                    .iter()
                    .find(|value| value.id == empty)
                    .unwrap()
                    .ty;
                function
                    .values
                    .push(psrs_backend::cc::ValueDecl { id: extra, ty });
                function.assignments.insert(
                    call_index,
                    Assignment {
                        destination: extra,
                        kind: A::VariantNew {
                            destination: extra,
                            representation,
                            case: 3,
                            fields: vec![extent, empty, extent],
                        },
                        span: function.span,
                    },
                );
                let A::DirectCall { arguments, .. } =
                    &mut function.assignments[call_index + 1].kind
                else {
                    unreachable!()
                };
                arguments[3] = extra;
            }
            _ => {
                let close_index = function
                    .assignments
                    .iter()
                    .position(|assignment| {
                        matches!(assignment.kind,
                    A::DirectCall { contract: Some(id), .. } if id == close)
                    })
                    .unwrap();
                let assignment = &mut function.assignments[close_index];
                match mutation {
                    Mutation::MissingClose => {
                        let ValueShape::Reference(reference) = extent_ty else {
                            unreachable!()
                        };
                        assignment.kind = A::RepresentationCast {
                            destination: assignment.destination,
                            value: extent,
                            reference,
                        };
                    }
                    Mutation::WrongClose => {
                        let A::DirectCall { arguments, .. } = &mut assignment.kind else {
                            unreachable!()
                        };
                        arguments[0] = extra;
                    }
                    Mutation::EarlyClose => {
                        let instruction = function.assignments.remove(close_index);
                        function.assignments.insert(call_index, instruction);
                    }
                    Mutation::CheckInsteadClose => {
                        let A::DirectCall {
                            contract, function, ..
                        } = &mut assignment.kind
                        else {
                            unreachable!()
                        };
                        *contract = Some(check_id);
                        *function = check;
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains(mutation.message())),
        "{errors:?}"
    );
}

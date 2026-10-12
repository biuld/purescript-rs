use super::{Mutation, R, SOURCE, binding};
use psrs_backend::cc::{Assignment, AssignmentKind as A, Function, RefShape, ValueId};

fn visit(body: &mut [Assignment], mutate: &mut impl FnMut(&mut Assignment) -> bool) -> bool {
    for assignment in body {
        if mutate(assignment) {
            return true;
        }
        if let A::TagSwitch {
            cases,
            default_assignments,
            ..
        } = &mut assignment.kind
        {
            for case in cases {
                if visit(&mut case.assignments, mutate) {
                    return true;
                }
            }
            if visit(default_assignments, mutate) {
                return true;
            }
        }
    }
    false
}

fn definition(function: &Function, value: ValueId) -> &A {
    fn find(body: &[Assignment], value: ValueId) -> Option<&A> {
        for assignment in body {
            if assignment.destination == value {
                return Some(&assignment.kind);
            }
            if let A::TagSwitch {
                cases,
                default_assignments,
                ..
            } = &assignment.kind
            {
                for case in cases {
                    if let Some(kind) = find(&case.assignments, value) {
                        return Some(kind);
                    }
                }
                if let Some(kind) = find(default_assignments, value) {
                    return Some(kind);
                }
            }
        }
        None
    }
    find(&function.assignments, value).unwrap()
}

pub(super) fn reject(mutation: Mutation) {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let role = mutation.role();
    let symbol = binding(&input.cc.representations, role).0;
    let check = binding(&input.cc.representations, R::Check).1;
    let close = binding(&input.cc.representations, R::Close);
    let helper = &input.cc.representations.control_helpers[check.0 as usize];
    let boundary_shape = input
        .cc
        .representations
        .signature(helper.signature())
        .unwrap()
        .result;
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let parameters = function.parameters.clone();
    let changed = match mutation {
        Mutation::PrematureClose => visit(&mut function.assignments, &mut |assignment| {
            let A::DirectCall {
                function,
                contract: Some(id),
                ..
            } = &mut assignment.kind
            else {
                return false;
            };
            if *id != check {
                return false;
            }
            *function = close.0;
            *id = close.1;
            true
        }),
        Mutation::CallbackFromOtherQueue => visit(&mut function.assignments, &mut |assignment| {
            let A::VariantGet {
                case: 1,
                field: 0,
                value,
                ..
            } = &mut assignment.kind
            else {
                return false;
            };
            *value = parameters[1];
            true
        }),
        Mutation::WrongSavedCheck | Mutation::MissingSavedCheck => {
            visit(&mut function.assignments, &mut |assignment| {
                let A::DirectCall {
                    contract: Some(id),
                    arguments,
                    ..
                } = &mut assignment.kind
                else {
                    return false;
                };
                if *id != check {
                    return false;
                }
                if matches!(mutation, Mutation::WrongSavedCheck) {
                    arguments[0] = parameters[4];
                } else {
                    let value = arguments[0];
                    assignment.kind = A::AggregateConvert {
                        destination: assignment.destination,
                        value,
                        conversion: psrs_backend::cc::AggregateConvert {
                            source: boundary_shape,
                            destination: boundary_shape,
                            plan: psrs_backend::cc::ValueConversion::Identity,
                        },
                    };
                }
                true
            })
        }
        Mutation::ReusedActivation | Mutation::WrongRestoredParent => {
            visit(&mut function.assignments, &mut |assignment| {
                let A::VariantNew {
                    case: 3, fields, ..
                } = &mut assignment.kind
                else {
                    return false;
                };
                if matches!(mutation, Mutation::ReusedActivation) {
                    fields[0] = parameters[2];
                } else {
                    fields[2] = parameters[3];
                }
                true
            })
        }
        Mutation::WrongActivationParent => {
            let psrs_backend::cc::ValueShape::Reference(reference) = boundary_shape else {
                unreachable!()
            };
            let RefShape::Repr(boundary) = reference.heap else {
                unreachable!()
            };
            visit(&mut function.assignments, &mut |assignment| {
                let A::VariantNew {
                    representation,
                    case: 1,
                    fields,
                    ..
                } = &mut assignment.kind
                else {
                    return false;
                };
                if *representation != boundary {
                    return false;
                }
                fields[1] = parameters[2];
                true
            })
        }
        Mutation::ClosedStatus => {
            let mut status = None;
            assert!(visit(&mut function.assignments, &mut |assignment| {
                if let A::ArrayNew { elements, .. } = &assignment.kind {
                    status = elements.first().copied();
                    return true;
                }
                false
            }));
            assert!(matches!(
                definition(function, status.unwrap()),
                A::Constant(0)
            ));
            visit(&mut function.assignments, &mut |assignment| {
                if assignment.destination == status.unwrap() {
                    assignment.kind = A::Constant(1);
                    return true;
                }
                false
            })
        }
        Mutation::WrongAdvance => {
            let mut finish = None;
            assert!(visit(&mut function.assignments, &mut |assignment| {
                if matches!(assignment.kind, A::VariantNew { case: 3, .. }) {
                    finish = Some(assignment.destination);
                    return true;
                }
                false
            }));
            visit(&mut function.assignments, &mut |assignment| {
                let A::DirectCall {
                    function: target,
                    arguments,
                    ..
                } = &mut assignment.kind
                else {
                    return false;
                };
                if *target != symbol || arguments[1] != finish.unwrap() {
                    return false;
                }
                arguments[2] = parameters[2];
                true
            })
        }
        Mutation::StatusOutsideLoop => unreachable!(),
    };
    assert!(changed);
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains(mutation.message())),
        "{errors:?}"
    );
}

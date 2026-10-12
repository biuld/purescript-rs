use super::{Mutation, R, SOURCE, binding};
use psrs_backend::cc::{Assignment, AssignmentKind as A, RefShape, ValueId, ValueShape};
use std::collections::HashMap;

fn index(body: &[Assignment], definitions: &mut HashMap<ValueId, A>) {
    for assignment in body {
        definitions.insert(assignment.destination, assignment.kind.clone());
        if let A::TagSwitch {
            cases,
            default_assignments,
            ..
        } = &assignment.kind
        {
            for case in cases {
                index(&case.assignments, definitions);
            }
            index(default_assignments, definitions);
        }
    }
}
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

pub(super) fn reject(mutation: Mutation) {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let symbol = binding(&input.cc.representations, mutation.role()).0;
    let clone = binding(&input.cc.representations, R::CloneSequence).1;
    let executor = binding(&input.cc.representations, R::Executor).1;
    let signature = input
        .cc
        .representations
        .signature(input.cc.representations.control_helpers[executor.0 as usize].signature())
        .unwrap();
    let repr = |shape| match shape {
        ValueShape::Reference(reference) => match reference.heap {
            RefShape::Repr(id) => id,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    let context = repr(signature.parameters[5]);
    let sequence = repr(signature.parameters[3]);
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let parameters = function.parameters.clone();
    let mut definitions = HashMap::new();
    index(&function.assignments, &mut definitions);
    let context_extent = |value| matches!(definitions.get(&value), Some(A::VariantGet { representation, case: 1, field: 5, .. }) if *representation == context);
    let target_finish = definitions.iter().find_map(|(value, kind)| match kind {
        A::VariantNew {
            representation,
            case: 3,
            fields,
            ..
        } if *representation == sequence && context_extent(fields[0]) => Some(*value),
        _ => None,
    });
    let changed = visit(
        &mut function.assignments,
        &mut |assignment| match mutation {
            Mutation::FinishExtent | Mutation::FinishQueue | Mutation::FinishParent => {
                if Some(assignment.destination) != target_finish {
                    return false;
                }
                let A::VariantNew { fields, .. } = &mut assignment.kind else {
                    unreachable!()
                };
                match mutation {
                    Mutation::FinishExtent => fields[0] = parameters[4],
                    Mutation::FinishQueue => fields[1] = parameters[3],
                    Mutation::FinishParent => fields[2] = parameters[4],
                    _ => unreachable!(),
                }
                true
            }
            Mutation::CallerContext => match &mut assignment.kind {
                A::DirectCall {
                    function: target,
                    arguments,
                    ..
                } if *target == symbol && Some(arguments[3]) == target_finish => {
                    arguments[5] = parameters[5];
                    true
                }
                _ => false,
            },
            Mutation::CopyParent => match &mut assignment.kind {
                A::VariantNew {
                    representation,
                    case: 3,
                    fields,
                    ..
                } if *representation == sequence && !context_extent(fields[0]) => {
                    fields[2] = parameters[4];
                    true
                }
                _ => false,
            },
            Mutation::CopyOrder => match &mut assignment.kind {
                A::VariantNew {
                    representation,
                    case: 2,
                    fields,
                    ..
                } if *representation == sequence
                    && matches!(
                        definitions.get(&fields[0]),
                        Some(A::VariantGet {
                            case: 3,
                            field: 1,
                            ..
                        })
                    ) =>
                {
                    fields.swap(0, 1);
                    true
                }
                _ => false,
            },
            Mutation::SeedParent | Mutation::SeedActive | Mutation::SeedAccumulator => {
                match &mut assignment.kind {
                    A::DirectCall {
                        contract: Some(id),
                        arguments,
                        ..
                    } if *id == clone => {
                        match mutation {
                            Mutation::SeedParent => arguments[4] = parameters[4],
                            Mutation::SeedActive => arguments[3] = arguments[4],
                            Mutation::SeedAccumulator => arguments[1] = arguments[0],
                            _ => unreachable!(),
                        }
                        true
                    }
                    _ => false,
                }
            }
            Mutation::CloneForward => match &mut assignment.kind {
                A::DirectCall {
                    function: target,
                    arguments,
                    ..
                } if *target == symbol => {
                    arguments[4] = parameters[3];
                    true
                }
                _ => false,
            },
            Mutation::CallbackForward => match &mut assignment.kind {
                A::DirectCall {
                    function: target,
                    arguments,
                    ..
                } if *target == symbol
                    && matches!(
                        definitions.get(&arguments[1]),
                        Some(A::VariantNew { case: 1, .. })
                    ) =>
                {
                    arguments[3] = parameters[2];
                    true
                }
                _ => false,
            },
        },
    );
    assert!(changed);
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains(mutation.message())),
        "{errors:?}"
    );
}

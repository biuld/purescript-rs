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
    let (symbol, id) = binding(&input.cc.representations, R::Executor);
    let signature = input
        .cc
        .representations
        .signature(input.cc.representations.control_helpers[id.0 as usize].signature())
        .unwrap();
    let ValueShape::Reference(reference) = signature.parameters[5] else {
        unreachable!()
    };
    let RefShape::Repr(context) = reference.heap else {
        unreachable!()
    };
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let parameters = function.parameters.clone();
    let mut definitions = HashMap::new();
    index(&function.assignments, &mut definitions);
    let tail = |fields: &[ValueId]| {
        matches!(
            definitions.get(&fields[5]),
            Some(A::VariantGet {
                case: 3,
                field: 0,
                ..
            })
        )
    };
    let context_value = definitions
        .iter()
        .find_map(|(value, kind)| match kind {
            A::VariantNew {
                representation,
                case: 1,
                fields,
                ..
            } if *representation == context && !tail(fields) => Some(*value),
            _ => None,
        })
        .unwrap();
    let status = match &definitions[&context_value] {
        A::VariantNew { fields, .. } => match &definitions[&fields[5]] {
            A::VariantNew { fields, .. } => fields[0],
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    let caller = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    if matches!(mutation, Mutation::Caller) {
        function.values.push(psrs_backend::cc::ValueDecl {
            id: caller,
            ty: signature.parameters[5],
        });
        function.assignments.insert(
            0,
            Assignment {
                destination: caller,
                kind: A::VariantGet {
                    destination: caller,
                    representation: context,
                    case: 1,
                    field: 4,
                    value: parameters[5],
                },
                span: function.span,
            },
        );
    }
    let changed = visit(&mut function.assignments, &mut |assignment| {
        match mutation {
            Mutation::Caller
            | Mutation::Clauses
            | Mutation::TailQueue
            | Mutation::TailParent
            | Mutation::TailExtent => {
                let A::VariantNew {
                    representation,
                    case: 1,
                    fields,
                    ..
                } = &mut assignment.kind
                else {
                    return false;
                };
                if *representation != context {
                    return false;
                }
                match mutation {
                    Mutation::Caller => fields[4] = caller,
                    Mutation::Clauses => fields.swap(0, 1),
                    _ if !tail(fields) => return false,
                    Mutation::TailQueue => fields[2] = parameters[3],
                    Mutation::TailParent => fields[3] = parameters[4],
                    Mutation::TailExtent => fields[5] = parameters[4],
                    _ => unreachable!(),
                }
            }
            Mutation::Status => {
                if assignment.destination != status {
                    return false;
                }
                let A::ArrayNew { elements, .. } = &mut assignment.kind else {
                    unreachable!()
                };
                elements.clear();
            }
            _ => {
                let A::DirectCall {
                    function: target,
                    arguments,
                    ..
                } = &mut assignment.kind
                else {
                    return false;
                };
                if *target != symbol || arguments[5] != context_value {
                    return false;
                }
                match mutation {
                    Mutation::EntryBody => arguments[2] = parameters[2],
                    Mutation::EntryQueue => arguments[3] = parameters[3],
                    Mutation::EntryExtent => arguments[4] = parameters[4],
                    Mutation::EntryContext => arguments[5] = parameters[5],
                    _ => unreachable!(),
                }
            }
        }
        true
    });
    assert!(changed);
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains(mutation.message())),
        "{errors:?}"
    );
}

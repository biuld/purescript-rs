use super::{Mutation, R, SOURCE, binding, path};
use psrs_backend::cc::{Assignment, AssignmentKind as A, Function, TagCase, ValueId};

fn arm<'a>(body: &'a mut [Assignment], path: &[i32]) -> &'a mut TagCase {
    let A::TagSwitch { cases, .. } = &mut body
        .iter_mut()
        .find(
            |assignment| matches!(&assignment.kind, A::TagSwitch { cases, .. } if cases.len() >= 2),
        )
        .unwrap()
        .kind
    else {
        unreachable!()
    };
    let case = cases.iter_mut().find(|case| case.tag == path[0]).unwrap();
    if path.len() == 1 {
        case
    } else {
        arm(&mut case.assignments, &path[1..])
    }
}

fn origin(function: &Function, mut value: ValueId) -> ValueId {
    let mut aliases = std::collections::HashMap::new();
    fn index(body: &[Assignment], aliases: &mut std::collections::HashMap<ValueId, ValueId>) {
        for assignment in body {
            match &assignment.kind {
                A::RepresentationCast { value, .. } => {
                    aliases.insert(assignment.destination, *value);
                }
                A::TagSwitch { cases, .. } => {
                    for case in cases {
                        index(&case.assignments, aliases)
                    }
                }
                _ => {}
            }
        }
    }
    index(&function.assignments, &mut aliases);
    for _ in 0..aliases.len() {
        if let Some(source) = aliases.get(&value) {
            value = *source
        } else {
            break;
        }
    }
    value
}

pub(super) fn reject(mutation: Mutation) {
    use Mutation::*;
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (symbol, _) = binding(&input.cc.representations, R::Executor);
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let parameters = function.parameters.clone();
    let signature = match function
        .values
        .iter()
        .find(|value| value.id == parameters[1])
        .unwrap()
        .ty
    {
        psrs_backend::cc::ValueShape::Reference(reference) => match reference.heap {
            psrs_backend::cc::RefShape::Closure(signature) => signature,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    let fresh = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function.values.push(psrs_backend::cc::ValueDecl {
        id: fresh,
        ty: function.result_type,
    });

    let returned = arm(&mut function.assignments, path(mutation)).value;
    let returned = origin(function, returned);
    let function_snapshot = function.clone();
    let case = arm(&mut function.assignments, path(mutation));
    let body = &mut case.assignments;
    let calls = body
        .iter()
        .filter_map(|assignment| {
            matches!(assignment.kind, A::IndirectCall { .. }).then_some(assignment.destination)
        })
        .collect::<Vec<_>>();
    match mutation {
        ExtraRootCall | ExtraQueueCall | EagerBind => {
            let kind = if matches!(mutation, EagerBind) {
                A::IndirectCall {
                    function: parameters[1],
                    signature,
                    arguments: vec![parameters[0]],
                }
            } else {
                body.iter()
                    .rfind(|assignment| matches!(assignment.kind, A::IndirectCall { .. }))
                    .unwrap()
                    .kind
                    .clone()
            };
            let position = body
                .iter()
                .position(|assignment| assignment.destination == returned)
                .unwrap();
            body.insert(
                position,
                Assignment {
                    destination: fresh,
                    kind,
                    span: function_snapshot.span,
                },
            );
        }

        HandlerState | Request | Handler | Raw | Callback | Answer => {
            let index = match mutation {
                HandlerState | Handler | Callback | Answer => 0,
                Request => 1,
                Raw => 2,
                _ => unreachable!(),
            };
            let raw = body
                .iter()
                .find(|assignment| matches!(assignment.kind, A::FunctionRef { .. }))
                .map(|assignment| assignment.destination);
            let replacement = if matches!(mutation, HandlerState) {
                match &body
                    .iter()
                    .find(|assignment| assignment.destination == calls[1])
                    .unwrap()
                    .kind
                {
                    A::IndirectCall { arguments, .. } => arguments[0],
                    _ => unreachable!(),
                }
            } else if matches!(mutation, Handler) {
                raw.unwrap()
            } else if matches!(mutation, Callback) {
                parameters[1]
            } else {
                parameters[0]
            };
            let A::IndirectCall {
                function: callee,
                arguments,
                ..
            } = &mut body
                .iter_mut()
                .find(|assignment| assignment.destination == calls[index])
                .unwrap()
                .kind
            else {
                unreachable!()
            };
            if matches!(mutation, Handler | Callback) {
                *callee = replacement
            } else {
                arguments[0] = replacement
            }
        }
        TrapRoot | TrapCons => {
            let assignment = body
                .iter_mut()
                .find(|assignment| assignment.destination == returned)
                .unwrap();
            assignment.kind = A::Unreachable;
        }
        ReturnState | ReturnAnswer => {
            let A::ProductNew { arguments, .. } = &mut body
                .iter_mut()
                .find(|assignment| matches!(assignment.kind, A::ProductNew { .. }))
                .unwrap()
                .kind
            else {
                unreachable!()
            };
            if matches!(mutation, ReturnState) {
                arguments[0] = arguments[1]
            } else {
                arguments[1] = parameters[0]
            }
        }
        _ => {
            let A::DirectCall { arguments, .. } = &body
                .iter()
                .find(|assignment| assignment.destination == returned)
                .unwrap()
                .kind
            else {
                panic!("transition")
            };
            let next = arguments.clone();
            match mutation {
                ResultState | ResultComputation | SourceQueue | Tail | EmptyAppend
                | QueueFinishRest | QueueFinishParent => {
                    let A::DirectCall { arguments, .. } = &mut body
                        .iter_mut()
                        .find(|assignment| assignment.destination == returned)
                        .unwrap()
                        .kind
                    else {
                        unreachable!()
                    };
                    let index = match mutation {
                        ResultState => 0,
                        ResultComputation => 2,
                        QueueFinishParent => 4,
                        _ => 3,
                    };
                    arguments[index] = parameters[index];
                }
                _ => {
                    let queue = origin(&function_snapshot, next[3]);
                    let A::VariantNew { fields, .. } = &body
                        .iter()
                        .find(|assignment| assignment.destination == queue)
                        .unwrap()
                        .kind
                    else {
                        panic!("new pending queue")
                    };
                    let fields = fields.clone();
                    match mutation {
                        AppendTail | AppendRight | FinishExtent | FinishParent => {
                            let A::VariantNew { fields, .. } = &mut body
                                .iter_mut()
                                .find(|assignment| assignment.destination == queue)
                                .unwrap()
                                .kind
                            else {
                                unreachable!()
                            };
                            match mutation {
                                AppendTail => fields[0] = parameters[3],
                                AppendRight => fields[1] = fields[0],
                                FinishExtent => fields[0] = fields[2],
                                FinishParent => fields[2] = fields[0],
                                _ => unreachable!(),
                            }
                        }
                        AppendFirst => {
                            let nested = origin(&function_snapshot, fields[1]);
                            let A::VariantNew { fields: inner, .. } = &body
                                .iter()
                                .find(|assignment| assignment.destination == nested)
                                .unwrap()
                                .kind
                            else {
                                unreachable!()
                            };
                            let second = inner[0];
                            let A::VariantNew { fields, .. } = &mut body
                                .iter_mut()
                                .find(|assignment| assignment.destination == queue)
                                .unwrap()
                                .kind
                            else {
                                unreachable!()
                            };
                            fields[0] = second;
                        }
                        AppendSecond | FinishRest => {
                            let nested = origin(&function_snapshot, fields[1]);
                            let A::VariantNew { fields: inner, .. } = &mut body
                                .iter_mut()
                                .find(|assignment| assignment.destination == nested)
                                .unwrap()
                                .kind
                            else {
                                unreachable!()
                            };
                            inner[0] = if matches!(mutation, AppendSecond) {
                                fields[0]
                            } else {
                                parameters[3]
                            };
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
    let errors = psrs_backend::cc::verify_module(&input.cc).expect_err("malformed root/queue step");
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P8 CC root and queue execution"
                || error.pass == "P8 CC root and queue effects"
                || (matches!(
                    mutation,
                    Mutation::QueueFinishRest | Mutation::QueueFinishParent
                ) && error.pass == "P8 CC control provenance")
                || (matches!(
                    mutation,
                    Mutation::FinishExtent | Mutation::FinishParent | Mutation::FinishRest
                ) && error.pass == "P8 CC finish creation")),
        "{mutation:?}: {errors:?}"
    );
}

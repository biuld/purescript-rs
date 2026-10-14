use super::{Mutation, R, SOURCE, binding};
use psrs_backend::cc::{Assignment, AssignmentKind as A, BinaryOp, ValueDecl, ValueId, ValueShape};

pub(super) fn check(mutation: Option<Mutation>, equality: bool) {
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
    let A::TagSwitch { cases, .. } = &mut function.assignments.last_mut().unwrap().kind else {
        panic!("executor input")
    };
    let case = cases.iter_mut().find(|case| case.tag == 5).unwrap();
    let body = &mut case.assignments;
    let stores = body
        .iter()
        .enumerate()
        .filter_map(|(index, assignment)| {
            matches!(assignment.kind, A::ArraySet { .. }).then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(stores.len(), 3);
    let reads = body
        .iter()
        .enumerate()
        .filter_map(|(index, assignment)| {
            matches!(assignment.kind, A::ArrayGet { .. }).then_some(index)
        })
        .collect::<Vec<_>>();
    let guard = body
        .iter()
        .position(|assignment| matches!(assignment.kind, A::TagSwitch { .. }))
        .unwrap();
    let call = body
        .iter()
        .position(|assignment| matches!(assignment.kind, A::IndirectCall { .. }))
        .unwrap();
    let [running, consumed] = [stores[0], stores[2]].map(|index| match body[index].kind {
        A::ArraySet { new_value, .. } => new_value,
        _ => unreachable!(),
    });
    let fresh = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    if equality {
        let old = body[guard].clone();
        let A::TagSwitch {
            value,
            cases,
            default_assignments,
            default_value,
        } = old.kind
        else {
            unreachable!()
        };
        let zero = fresh;
        let predicate = ValueId(fresh.0 + 1);
        function.values.extend([
            ValueDecl {
                id: zero,
                ty: ValueShape::Integer,
            },
            ValueDecl {
                id: predicate,
                ty: ValueShape::Boolean,
            },
        ]);
        body.splice(
            guard..=guard,
            [
                Assignment {
                    destination: zero,
                    kind: A::Constant(0),
                    span: old.span,
                },
                Assignment {
                    destination: predicate,
                    kind: A::Primitive {
                        op: BinaryOp::IntEq,
                        left: value,
                        right: zero,
                    },
                    span: old.span,
                },
                Assignment {
                    destination: old.destination,
                    kind: A::If {
                        condition: predicate,
                        then_assignments: cases[0].assignments.clone(),
                        then_value: cases[0].value,
                        else_assignments: default_assignments,
                        else_value: default_value,
                    },
                    span: old.span,
                },
            ],
        );
    }
    if let Some(mutation) = mutation {
        match mutation {
            Guard => {
                body.remove(guard);
            }
            GuardTag => {
                let A::TagSwitch { cases, .. } = &mut body[guard].kind else {
                    unreachable!()
                };
                cases[0].tag = 1;
            }
            Running | Clear | Consumed => {
                body.remove(
                    stores[match mutation {
                        Running => 0,
                        Clear => 1,
                        _ => 2,
                    }],
                );
            }
            RunningValue | ConsumedValue | StateOrder => {
                for assignment in body {
                    if assignment.destination == running
                        && matches!(mutation, RunningValue | StateOrder)
                    {
                        assignment.kind =
                            A::Constant(if matches!(mutation, StateOrder) { 2 } else { 3 });
                    }
                    if assignment.destination == consumed
                        && matches!(mutation, ConsumedValue | StateOrder)
                    {
                        assignment.kind =
                            A::Constant(if matches!(mutation, StateOrder) { 1 } else { 3 });
                    }
                }
            }
            ClearValue => {
                let result = body[reads[1]].destination;
                let A::ArraySet { new_value, .. } = &mut body[stores[1]].kind else {
                    unreachable!()
                };
                *new_value = result;
            }
            ReadOrder => {
                let A::ArrayGet { index: zero, .. } = body[reads[0]].kind else {
                    unreachable!()
                };
                let mut read = body.remove(reads[1]);
                let A::ArrayGet { index, .. } = &mut read.kind else {
                    unreachable!()
                };
                *index = zero;
                body.insert(stores[0], read);
            }
            Callee | Answer => {
                let A::IndirectCall {
                    function: callee,
                    arguments,
                    ..
                } = &mut body[call].kind
                else {
                    unreachable!()
                };
                if matches!(mutation, Callee) {
                    *callee = parameters[1]
                } else {
                    arguments[0] = parameters[0]
                }
            }
            Result => {
                let A::DirectCall { arguments, .. } = &mut body
                    .iter_mut()
                    .find(|assignment| matches!(assignment.kind, A::DirectCall { .. }))
                    .unwrap()
                    .kind
                else {
                    unreachable!()
                };
                arguments[2] = parameters[2];
            }
            ExtraCall => {
                let mut extra = body[call].clone();
                let ty = function
                    .values
                    .iter()
                    .find(|value| value.id == extra.destination)
                    .unwrap()
                    .ty;
                extra.destination = fresh;
                function.values.push(ValueDecl { id: fresh, ty });
                body.insert(call + 1, extra);
            }
            ExtraStore => {
                let mut extra = body[stores[2]].clone();
                let A::ArrayGet { index: zero, .. } = body[reads[0]].kind else {
                    unreachable!()
                };
                let A::ArraySet { new_value, .. } = &mut extra.kind else {
                    unreachable!()
                };
                *new_value = zero;
                body.insert(stores[2] + 1, extra);
            }
        }
    }
    let result = psrs_backend::cc::verify_module(&input.cc);
    if mutation.is_some() {
        let errors = result.expect_err("malformed owned transfer");
        assert!(
            errors
                .iter()
                .any(|error| error.pass == "P8 CC owned execution"),
            "{mutation:?}: {errors:?}"
        );
    } else {
        result.expect("equivalent Pending predicate");
        psrs_backend::compile_cc(input).expect("equivalent Pending guard compiles through MIR");
    }
}

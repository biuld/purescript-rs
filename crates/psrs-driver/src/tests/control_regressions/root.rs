//! Root lifetime belongs to the same checked construction boundary as replay.
use super::{SOURCE, binding};
use psrs_backend::cc::{AssignmentKind as A, ControlHelperRole as R};

fn normal(function: &mut psrs_backend::cc::Function) -> &mut Vec<psrs_backend::cc::Assignment> {
    let A::Protect { body, .. } = &mut function.assignments.last_mut().expect("guarded Run").kind
    else {
        panic!("Run protects execution and normal closure")
    };
    &mut body.assignments
}

#[test]
fn cc_run_cannot_check_instead_of_closing_its_root() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (close, _) = binding(&input.cc.representations, R::Close);
    let (check, check_id) = binding(&input.cc.representations, R::Check);
    let (run, _) = binding(&input.cc.representations, R::Run);
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|f| f.symbol == run)
        .unwrap();
    let assignment = normal(function)
        .iter_mut()
        .find(|a| matches!(a.kind, A::DirectCall { function, .. } if function == close))
        .unwrap();
    let A::DirectCall {
        function, contract, ..
    } = &mut assignment.kind
    else {
        unreachable!()
    };
    *function = check;
    *contract = Some(check_id);
    let errors = psrs_backend::cc::verify_module(&input.cc)
        .expect_err("Run cannot publish a result without closing its own root");
    assert!(
        errors.iter().any(|e| e
            .message
            .contains("emitted machine operands or ordered operations")),
        "{errors:?}"
    );
}

#[test]
fn cc_run_requires_its_fresh_root_empty_queue_ordered_close_and_result() {
    use psrs_backend::cc::{Assignment, Function, ValueDecl, ValueId};
    #[derive(Clone, Copy, Debug)]
    enum Mutation {
        MissingClose,
        EarlyClose,
        WrongResult,
        ExecutionRoot,
        DispatchRoot,
        CloseRoot,
        NonemptyQueue,
    }
    use Mutation::*;
    for mutation in [
        MissingClose,
        EarlyClose,
        WrongResult,
        ExecutionRoot,
        DispatchRoot,
        CloseRoot,
        NonemptyQueue,
    ] {
        let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
        let (run, _) = binding(&input.cc.representations, R::Run);
        let (start, start_id) = binding(&input.cc.representations, R::Start);
        let (executor, _) = binding(&input.cc.representations, R::Executor);
        let (close, _) = binding(&input.cc.representations, R::Close);
        let f: &mut Function = input
            .cc
            .functions
            .iter_mut()
            .find(|f| f.symbol == run)
            .unwrap();
        let call_index = normal(f)
            .iter()
            .position(|a| matches!(a.kind, A::DirectCall { function, .. } if function == executor))
            .unwrap();
        let A::DirectCall { arguments, .. } = &normal(f)[call_index].kind else {
            unreachable!()
        };
        let (queue, root, context) = (arguments[3], arguments[4], arguments[5]);
        let close_index = normal(f)
            .iter()
            .position(|a| matches!(a.kind, A::DirectCall { function, .. } if function == close))
            .unwrap();
        match mutation {
            MissingClose => {
                normal(f).remove(close_index);
            }
            EarlyClose => {
                let close = normal(f).remove(close_index);
                normal(f).insert(call_index, close);
            }
            WrongResult => {
                f.result = f.parameters[0];
            }
            ExecutionRoot | DispatchRoot | CloseRoot => {
                let extra = ValueId(f.values.iter().map(|v| v.id.0).max().unwrap() + 1);
                let ty = f.values.iter().find(|v| v.id == root).unwrap().ty;
                f.values.push(ValueDecl { id: extra, ty });
                f.assignments.insert(
                    0,
                    Assignment {
                        destination: extra,
                        kind: A::DirectCall {
                            contract: Some(start_id),
                            function: start,
                            arguments: vec![],
                        },
                        span: f.span,
                    },
                );
                if matches!(mutation, ExecutionRoot) {
                    let A::DirectCall { arguments, .. } = &mut normal(f)[call_index].kind else {
                        unreachable!()
                    };
                    arguments[4] = extra;
                } else if matches!(mutation, DispatchRoot) {
                    let A::VariantNew { fields, .. } = &mut f
                        .assignments
                        .iter_mut()
                        .find(|a| a.destination == context)
                        .unwrap()
                        .kind
                    else {
                        unreachable!()
                    };
                    fields[0] = extra;
                } else {
                    let A::DirectCall { arguments, .. } = &mut normal(f)[close_index].kind else {
                        unreachable!()
                    };
                    arguments[0] = extra;
                }
            }
            NonemptyQueue => {
                let A::VariantNew { case, fields, .. } = &mut f
                    .assignments
                    .iter_mut()
                    .find(|a| a.destination == queue)
                    .unwrap()
                    .kind
                else {
                    unreachable!()
                };
                *case = 2; // Append(Empty, Empty) has no callbacks but is not Empty.
                fields.extend([queue, queue]);
                // Replace self-references with a separate valid Empty, so this
                // exercises the machine relation rather than use-before-bind.
                let empty = ValueId(f.values.iter().map(|v| v.id.0).max().unwrap() + 1);
                let ty = f.values.iter().find(|v| v.id == queue).unwrap().ty;
                f.values.push(ValueDecl { id: empty, ty });
                let mut allocation = f
                    .assignments
                    .iter()
                    .find(|a| a.destination == queue)
                    .unwrap()
                    .clone();
                let A::VariantNew {
                    destination,
                    case,
                    fields,
                    ..
                } = &mut allocation.kind
                else {
                    unreachable!()
                };
                *destination = empty;
                *case = 0;
                fields.clear();
                allocation.destination = empty;
                let index = f
                    .assignments
                    .iter()
                    .position(|a| a.destination == queue)
                    .unwrap();
                f.assignments.insert(index, allocation);
                let A::VariantNew { fields, .. } = &mut f.assignments[index + 1].kind else {
                    unreachable!()
                };
                *fields = vec![empty, empty];
            }
        }
        let errors = psrs_backend::cc::verify_module(&input.cc)
            .expect_err("malformed Run must fail P8 construction verification");
        assert!(
            errors.iter().any(|e| e
                .message
                .contains("emitted machine operands or ordered operations")),
            "mutation {mutation:?}: {errors:?}"
        );
    }
}

#[test]
fn sequential_and_exclusive_runs_preserve_independent_invocations_after_optimization() {
    const HEADER: &str = r"module Main where
import Prim.Control as C
data Op x = Op (Int -> x)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler state (Op k) raw = { _1: state, _2: raw (k 42) }
";
    for body in [
        "intAdd ((C.run 0 handler (C.return 20))._2) ((C.run 0 handler (C.return 22))._2)",
        r"let invoke = \flag -> if flag then (C.run 0 handler (C.return 20))._2 else (C.run 0 handler (C.return 22))._2 in intAdd (invoke true) (invoke false)",
    ] {
        let source = format!("{HEADER}\nmain :: Int\nmain = {body}\n");
        let Some(output) = super::super::run_with_wasmtime(&source) else {
            return;
        };
        assert_eq!(output.status.code(), Some(42), "{output:?}");
    }
}

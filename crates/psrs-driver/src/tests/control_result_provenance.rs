use psrs_backend::cc::{AssignmentKind as A, ControlHelperRole as R};
use psrs_backend::mir::{Instruction as I, Terminator as T};
const SOURCE: &str = r"module Main where
import Prim.Control as C
data Op x = Op (Int -> x)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler state (Op k) raw = { _1: state, _2: raw (k 42) }
main :: Int
main = (C.run 0 handler (C.bind (C.perform (Op \x -> x)) (\_ -> C.return 43)))._2
";
fn symbol(input: &psrs_backend::BackendInput) -> psrs_hir::SymbolId {
    input
        .cc
        .representations
        .control_helpers
        .iter()
        .find(|h| h.role() == R::Executor)
        .unwrap()
        .function()
}
#[test]
fn cc_cannot_discard_outer_return_result() {
    for tag in 0..=5 {
        let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
        let symbol = symbol(&input);
        let function = input
            .cc
            .functions
            .iter_mut()
            .find(|function| function.symbol == symbol)
            .unwrap();
        let state = function.parameters[0];
        let A::TagSwitch { cases, .. } = &mut function.assignments.last_mut().unwrap().kind else {
            panic!("executor input dispatch")
        };
        cases.iter_mut().find(|case| case.tag == tag).unwrap().value = state;
        let errors =
            psrs_backend::cc::verify_module(&input.cc).expect_err("discarded machine transition");
        assert!(
            errors.iter().any(|error| error
                .message
                .contains("executor transition result must reach every normal exit")),
            "tag {tag}: {errors:?}"
        );
        assert!(psrs_backend::compile_cc(input).is_err());
    }
}

fn joined_input(same: bool) -> psrs_backend::BackendInput {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let symbol = symbol(&input);
    let f = input
        .cc
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    fn wrap(
        body: &mut [psrs_backend::cc::Assignment],
        next: &mut u32,
        values: &mut Vec<psrs_backend::cc::ValueDecl>,
        alternative: Option<psrs_backend::cc::ValueId>,
    ) -> bool {
        for a in body {
            match &mut a.kind {
                A::TagSwitch { cases, .. }
                    if cases.len() == 2 && cases[0].tag == 0 && cases[1].tag == 1 =>
                {
                    let case = &mut cases[1];
                    let original = case.value;
                    let ty = values.iter().find(|v| v.id == original).unwrap().ty;
                    let cond = psrs_backend::cc::ValueId(*next);
                    *next += 1;
                    let result = psrs_backend::cc::ValueId(*next);
                    *next += 1;
                    values.push(psrs_backend::cc::ValueDecl {
                        id: cond,
                        ty: psrs_backend::cc::ValueShape::Boolean,
                    });
                    values.push(psrs_backend::cc::ValueDecl { id: result, ty });
                    case.assignments.push(psrs_backend::cc::Assignment {
                        destination: cond,
                        kind: A::Constant(1),
                        span: a.span,
                    });
                    case.assignments.push(psrs_backend::cc::Assignment {
                        destination: result,
                        kind: A::If {
                            condition: cond,
                            then_assignments: vec![],
                            then_value: original,
                            else_assignments: vec![],
                            else_value: alternative.unwrap_or(original),
                        },
                        span: a.span,
                    });
                    case.value = result;
                    return true;
                }
                A::TagSwitch { cases, .. } => {
                    for case in cases {
                        if wrap(&mut case.assignments, next, values, alternative) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }
    let A::TagSwitch { cases, .. } = &mut f.assignments.last_mut().unwrap().kind else {
        panic!()
    };
    let body = &mut cases.iter_mut().find(|c| c.tag == 0).unwrap().assignments;
    let mut next = f.values.iter().map(|v| v.id.0).max().unwrap() + 1;
    let alternative = (!same).then_some(f.parameters[0]);
    assert!(wrap(body, &mut next, &mut f.values, alternative));
    input
}

#[test]
fn cc_equivalent_scoped_result_join() {
    let input = joined_input(true);
    psrs_backend::cc::verify_module(&input.cc).expect("equivalent result join");
    let artifact =
        psrs_backend::compile_cc(input).expect("equivalent result join compiles through MIR");
    if super::wasmtime_available().is_some() {
        let path = std::env::temp_dir().join("psrs-control-equivalent-result-join.wasm");
        std::fs::write(&path, artifact.wasm).unwrap();
        let output = std::process::Command::new("wasmtime")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(43), "{output:?}");
    }
}

#[test]
fn cc_result_join_cannot_hide_a_discarded_alternative() {
    let input = joined_input(false);
    let errors =
        psrs_backend::cc::verify_module(&input.cc).expect_err("one result alternative discarded");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("executor transition result must reach every normal exit")),
        "{errors:?}"
    );
}
#[test]
fn mir_wrong_machine_constructor_tag() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let exec = symbol(&input);
    let mut module = crate::compile_main_stages(SOURCE).unwrap().mir;
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.symbol == exec)
        .unwrap();
    let target = f
        .blocks
        .iter()
        .find_map(|b| match &b.terminator {
            Some(T::Switch { cases, .. })
                if cases.iter().map(|(t, _)| *t).collect::<Vec<_>>() == vec![0, 1, 2, 3, 4, 5] =>
            {
                Some(cases.iter().find(|(t, _)| *t == 2).unwrap().1)
            }
            _ => None,
        })
        .unwrap();
    let tag = f
        .blocks
        .iter()
        .find(|b| b.id == target)
        .unwrap()
        .instructions
        .iter()
        .find_map(|i| match i {
            I::StructNew { arguments, .. } if arguments.len() == 3 => Some(arguments[0]),
            _ => None,
        })
        .unwrap();
    let definition = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| i.destination() == Some(tag))
        .unwrap();
    let I::Constant { value, .. } = definition else {
        panic!("tag constant")
    };
    *value = 0;
    let errors = psrs_backend::mir::verify_module(&module).expect_err("wrong nominal case tag");
    assert!(
        errors.iter().any(|e| e
            .message
            .contains("variant constructor must initialize its planned case tag")),
        "{errors:?}"
    );
}

#[test]
fn mir_executor_call_must_return_its_actual_result() {
    for discard in [false, true] {
        let input = crate::lower_main_to_cc(SOURCE).unwrap();
        let (symbol, contract) =
            super::control_checkpoints::binding(&input.cc.representations, R::Executor);
        let mut module = crate::compile_main_stages(SOURCE).unwrap().mir;
        let function = module
            .functions
            .iter_mut()
            .find(|f| f.symbol == symbol)
            .unwrap();
        let header = match function
            .blocks
            .iter()
            .find(|b| b.id == function.entry)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
        {
            T::Jump { target, .. } => *target,
            _ => panic!("executor loop preheader"),
        };
        let result = psrs_backend::types::ValueId(
            function
                .values
                .iter()
                .map(|value| value.id.0)
                .max()
                .unwrap()
                + 1,
        );
        function.values.push(psrs_backend::types::ValueDecl {
            id: result,
            ty: function.result_type,
        });
        let block = function.blocks.iter_mut().find(|block| {
            block.instructions.iter().any(|instruction| matches!(instruction, I::ClosureCall { .. }))
                && matches!(&block.terminator, Some(T::Jump { target, arguments, .. }) if *target == header && arguments.len() == 6)
        }).unwrap();
        let T::Jump {
            arguments, span, ..
        } = block.terminator.take().unwrap()
        else {
            unreachable!()
        };
        let state = arguments[0];
        block.instructions.push(I::Call {
            destination: result,
            contract: Some(contract),
            function: symbol,
            arguments,
            span,
        });
        block.terminator = Some(T::Return {
            value: if discard { state } else { result },
            span,
        });
        let verified = psrs_backend::mir::verify_module(&module);
        if discard {
            let errors = verified.expect_err("call result discarded at actual return");
            assert!(
                errors.iter().any(|error| error
                    .message
                    .contains("executor transition result must reach every normal exit")),
                "{errors:?}"
            );
        } else {
            verified.expect("actual executor result returned");
        }
    }
}

#[test]
fn cc_equivalent_switch_result_join() {
    use psrs_backend::cc::{Assignment, TagCase, ValueDecl, ValueId, ValueShape};
    let mut input = joined_input(true);
    let symbol = symbol(&input);
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    let trap = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function.values.push(ValueDecl {
        id: trap,
        ty: function.result_type,
    });
    let mut selector = None;
    assert!(super::control_dispatch::visit(
        &mut function.assignments,
        &mut |assignment| {
            let A::If {
                condition,
                then_value,
                else_value,
                ..
            } = &assignment.kind
            else {
                return false;
            };
            if then_value != else_value {
                return false;
            }
            selector = Some(*condition);
            let returned = *then_value;
            assignment.kind = A::TagSwitch {
                value: *condition,
                cases: vec![
                    TagCase {
                        tag: 0,
                        assignments: vec![],
                        value: returned,
                    },
                    TagCase {
                        tag: 1,
                        assignments: vec![],
                        value: returned,
                    },
                ],
                default_assignments: vec![Assignment {
                    destination: trap,
                    kind: A::Unreachable,
                    span: assignment.span,
                }],
                default_value: trap,
            };
            true
        }
    ));
    function
        .values
        .iter_mut()
        .find(|value| Some(value.id) == selector)
        .unwrap()
        .ty = ValueShape::Integer;
    psrs_backend::cc::verify_module(&input.cc).expect("equivalent switch result join");
    psrs_backend::compile_cc(input).expect("equivalent switch result join compiles through MIR");
}

//! Physical inlined factory cells are checked at the source publication witness.
use super::control_checkpoints::SOURCE;
use psrs_backend::mir::Instruction as I;
use psrs_backend::types::{ValueDecl, ValueId, ValueType};
use psrs_hir::Intrinsic;
use std::collections::HashMap;

fn origin(definitions: &HashMap<ValueId, I>, mut value: ValueId) -> ValueId {
    for _ in 0..definitions.len() {
        match definitions.get(&value) {
            Some(I::Copy { value: next, .. } | I::RefCast { value: next, .. }) => value = *next,
            _ => break,
        }
    }
    value
}

#[test]
fn inlined_start_and_once_check_current_contents_at_publication() {
    let owned = SOURCE.replace(
        "main = (C.run 0 handler (C.perform (Op \\x -> x)))._2",
        "main = let owner = C.once (\\n -> C.return n) in (C.run 0 handler (C.resume owner 42))._2",
    );
    for operation in [Intrinsic::ControlRun, Intrinsic::ControlOnce] {
        for final_status in [0, 2] {
            let mut stages = crate::compile_main_stages(if operation == Intrinsic::ControlRun {
                SOURCE
            } else {
                &owned
            })
            .unwrap();
            let table = &stages.cc.representations;
            let function = stages.mir.functions.iter_mut().find(|f| f.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i, I::ControlEvidence { contract, .. } if table.control_contracts[contract.0 as usize].operation() == operation))).unwrap();
            let definitions = function
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .filter_map(|i| i.destination().map(|v| (v, i.clone())))
                .collect::<HashMap<_, _>>();
            let (implementation, witness) = function
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| match i {
                    I::ControlEvidence {
                        contract,
                        implementation,
                        ..
                    } if table.control_contracts[contract.0 as usize].operation() == operation => {
                        Some((
                            origin(&definitions, *implementation),
                            i.destination().unwrap(),
                        ))
                    }
                    _ => None,
                })
                .unwrap();
            let frame = if operation == Intrinsic::ControlRun {
                let I::Call { arguments, .. } = &definitions[&implementation] else {
                    panic!("executor")
                };
                origin(&definitions, arguments[4])
            } else {
                implementation
            };
            let I::StructNew { arguments, .. } = &definitions[&frame] else {
                panic!("inlined factory")
            };
            let cell = origin(
                &definitions,
                arguments[if operation == Intrinsic::ControlRun {
                    1
                } else {
                    2
                }],
            );
            let I::ArrayNew { type_index, .. } = &definitions[&cell] else {
                panic!("fresh status")
            };
            let fresh = ValueId(function.values.iter().map(|v| v.id.0).max().unwrap() + 1);
            function.values.extend([
                ValueDecl {
                    id: fresh,
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(fresh.0 + 1),
                    ty: ValueType::I32,
                },
            ]);
            let point = if operation == Intrinsic::ControlRun {
                implementation
            } else {
                witness
            };
            let block = function
                .blocks
                .iter_mut()
                .find(|b| {
                    b.instructions
                        .iter()
                        .any(|i| i.destination() == Some(point))
                })
                .unwrap();
            let index = block
                .instructions
                .iter()
                .position(|i| i.destination() == Some(point))
                .unwrap();
            let span = function.span;
            block.instructions.splice(
                index..index,
                [
                    I::Constant {
                        destination: fresh,
                        value: final_status,
                        span,
                    },
                    I::Constant {
                        destination: ValueId(fresh.0 + 1),
                        value: 0,
                        span,
                    },
                    I::ArraySet {
                        type_index: *type_index,
                        value: cell,
                        index: ValueId(fresh.0 + 1),
                        new_value: fresh,
                        span,
                    },
                ],
            );
            let verdict = psrs_backend::mir::verify_module(&stages.mir);
            if final_status == 0 {
                verdict.expect("current published state remains live");
            } else {
                let errors = verdict.expect_err("inlined factory published a spent cell");
                assert!(
                    errors
                        .iter()
                        .any(|e| e.pass == "P9 MIR control publication"),
                    "{operation:?}: {errors:?}"
                );
            }
        }
    }
}

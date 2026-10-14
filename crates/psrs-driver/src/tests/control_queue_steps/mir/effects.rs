use super::{R, SOURCE, binding, selected};
use psrs_backend::mir::{Instruction as I, Terminator as T};
use psrs_backend::types::ValueId;

#[test]
fn mir_machine_paths_reject_additional_writes_and_traps() {
    use psrs_backend::types::{MemoryId, ValueDecl, ValueType};
    for kind in ["store", "trap"] {
        for location in ["return", "default", "entry", "publication"] {
            let input = crate::lower_main_to_cc(SOURCE).unwrap();
            let role = if location == "publication" {
                R::Start
            } else {
                R::Executor
            };
            let (symbol, _) = binding(&input.cc.representations, role);
            let mut module = if location == "publication" {
                psrs_backend::mir::lower_module(input.cc).unwrap().0
            } else {
                crate::compile_main_stages(SOURCE).unwrap().mir
            };
            let function = module
                .functions
                .iter_mut()
                .find(|f| f.symbol == symbol)
                .unwrap();
            let id = if matches!(location, "entry" | "publication") {
                function.entry
            } else {
                let T::Jump { target, .. } = function
                    .blocks
                    .iter()
                    .find(|b| b.id == function.entry)
                    .unwrap()
                    .terminator
                    .as_ref()
                    .unwrap()
                else {
                    panic!("header")
                };
                if location == "return" {
                    selected(function, *target, &[0, 0, 0])
                } else {
                    let T::Switch { default, .. } = function
                        .blocks
                        .iter()
                        .find(|b| b.id == *target)
                        .unwrap()
                        .terminator
                        .as_ref()
                        .unwrap()
                    else {
                        panic!("dispatch")
                    };
                    *default
                }
            };
            let value = ValueId(function.values.iter().map(|v| v.id.0).max().unwrap() + 1);
            let ty = if kind == "store" {
                ValueType::I32
            } else {
                ValueType::Boolean
            };
            function.values.push(ValueDecl { id: value, ty });
            let span = function.span;
            let block = function.blocks.iter_mut().find(|b| b.id == id).unwrap();
            block.instructions.splice(
                0..0,
                [
                    I::Constant {
                        destination: value,
                        value: 1,
                        span,
                    },
                    if kind == "store" {
                        I::Store {
                            address: value,
                            value,
                            memory: MemoryId(0),
                            offset: 0,
                            span,
                        }
                    } else {
                        I::TrapIf {
                            condition: value,
                            span,
                        }
                    },
                ],
            );
            let errors =
                psrs_backend::mir::verify_module(&module).expect_err("unaccounted operation");
            assert!(
                errors.iter().any(|e| matches!(
                    e.pass,
                    "P9 MIR root and queue execution" | "P9 MIR control initialization"
                )),
                "{kind}/{location}: {errors:?}"
            );
        }
    }
}

#[test]
fn mir_executor_rejects_an_unused_narrowing_cast_of_erased_state() {
    use psrs_backend::types::{ValueDecl, ValueType};
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (symbol, _) = binding(&input.cc.representations, R::Executor);
    let mut module = crate::compile_main_stages(SOURCE).unwrap().mir;
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    let T::Jump { target, .. } = function
        .blocks
        .iter()
        .find(|b| b.id == function.entry)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
    else {
        panic!("header")
    };
    let parameters = &function
        .blocks
        .iter()
        .find(|b| b.id == *target)
        .unwrap()
        .parameters;
    let state = parameters[0];
    let ValueType::Ref(reference) = function
        .values
        .iter()
        .find(|v| v.id == parameters[2])
        .unwrap()
        .ty
    else {
        panic!("computation")
    };
    let id = selected(function, *target, &[0, 0, 0]);
    let value = ValueId(function.values.iter().map(|v| v.id.0).max().unwrap() + 1);
    function.values.push(ValueDecl {
        id: value,
        ty: ValueType::Ref(reference),
    });
    function
        .blocks
        .iter_mut()
        .find(|b| b.id == id)
        .unwrap()
        .instructions
        .push(I::RefCast {
            destination: value,
            value: state,
            reference,
            span: function.span,
        });
    let errors =
        psrs_backend::mir::verify_module(&module).expect_err("an unused cast may still trap");
    assert!(
        errors
            .iter()
            .any(|e| e.pass == "P9 MIR root and queue execution"),
        "{errors:?}"
    );
}

#[test]
fn mir_failed_pending_guard_cannot_start_an_owned_transfer_before_trapping() {
    use psrs_backend::types::{ValueDecl, ValueType};
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (symbol, _) = binding(&input.cc.representations, R::Executor);
    let mut module = crate::compile_main_stages(SOURCE).unwrap().mir;
    let function = module
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    let T::Jump { target, .. } = function
        .blocks
        .iter()
        .find(|b| b.id == function.entry)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
    else {
        panic!("header")
    };
    let owned = selected(function, *target, &[5]);
    let guard = function.blocks.iter().find(|b| b.id == owned).unwrap();
    let T::Switch { default, .. } = guard.terminator.as_ref().unwrap() else {
        panic!("guard")
    };
    let failure = *default;
    let (cell, type_index, index) = guard
        .instructions
        .iter()
        .find_map(|i| match i {
            I::ArrayGet {
                value,
                type_index,
                index,
                ..
            } => Some((*value, *type_index, *index)),
            _ => None,
        })
        .unwrap();
    let fresh = ValueId(function.values.iter().map(|v| v.id.0).max().unwrap() + 1);
    function.values.push(ValueDecl {
        id: fresh,
        ty: ValueType::I32,
    });
    let span = function.span;
    function
        .blocks
        .iter_mut()
        .find(|b| b.id == failure)
        .unwrap()
        .instructions
        .splice(
            0..0,
            [
                I::Constant {
                    destination: fresh,
                    value: 1,
                    span,
                },
                I::ArraySet {
                    type_index,
                    value: cell,
                    index,
                    new_value: fresh,
                    span,
                },
            ],
        );
    let errors = psrs_backend::mir::verify_module(&module)
        .expect_err("a failed acquisition must trap before any transfer");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("exceptional machine path")),
        "{errors:?}"
    );
}

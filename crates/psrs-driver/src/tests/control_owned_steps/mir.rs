use super::{Mutation, R, SOURCE, binding};
use psrs_backend::mir::{Instruction as I, NumericOp, Terminator as T};
use psrs_backend::types::{ValueDecl, ValueId, ValueType};

pub(super) fn check(mutation: Option<Mutation>, equality: bool) {
    use Mutation::*;
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (symbol, _) = binding(&input.cc.representations, R::Executor);
    let mut module = crate::compile_main_stages(SOURCE).unwrap().mir;
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let header = match function
        .blocks
        .iter()
        .find(|block| block.id == function.entry)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
    {
        T::Jump { target, .. } => *target,
        _ => panic!("loop header"),
    };
    let parameters = function
        .blocks
        .iter()
        .find(|block| block.id == header)
        .unwrap()
        .parameters
        .clone();
    let owned = function
        .blocks
        .iter()
        .find_map(|block| match &block.terminator {
            Some(T::Switch { cases, .. })
                if cases.iter().map(|(tag, _)| *tag).collect::<Vec<_>>()
                    == vec![0, 1, 2, 3, 4, 5] =>
            {
                Some(cases.iter().find(|(tag, _)| *tag == 5).unwrap().1)
            }
            _ => None,
        })
        .unwrap();
    let (guard_id, mut transfer_id) = {
        let block = function
            .blocks
            .iter()
            .find(|block| block.id == owned)
            .unwrap();
        let T::Switch { cases, .. } = block.terminator.as_ref().unwrap() else {
            panic!("Pending guard")
        };
        (owned, cases[0].1)
    };
    loop {
        let block = function
            .blocks
            .iter()
            .find(|block| block.id == transfer_id)
            .unwrap();
        if block
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, I::ArraySet { .. }))
        {
            break;
        }
        let T::Jump { target, .. } = block.terminator.as_ref().unwrap() else {
            panic!("guard join")
        };
        transfer_id = *target;
    }
    let status_index = function
        .blocks
        .iter()
        .find(|block| block.id == guard_id)
        .unwrap()
        .instructions
        .iter()
        .find_map(|instruction| match instruction {
            I::ArrayGet { index, .. } => Some(*index),
            _ => None,
        })
        .unwrap();
    let fresh = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    let span = function.span;
    if equality {
        function.values.extend([
            ValueDecl {
                id: fresh,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(fresh.0 + 1),
                ty: ValueType::Boolean,
            },
        ]);
        let block = function
            .blocks
            .iter_mut()
            .find(|block| block.id == guard_id)
            .unwrap();
        let T::Switch {
            value,
            cases,
            default,
            span,
        } = block.terminator.take().unwrap()
        else {
            unreachable!()
        };
        block.instructions.extend([
            I::Constant {
                destination: fresh,
                value: 0,
                span,
            },
            I::Primitive {
                destination: ValueId(fresh.0 + 1),
                op: NumericOp::I32Eq,
                left: value,
                right: fresh,
                span,
            },
        ]);
        block.terminator = Some(T::Branch {
            condition: ValueId(fresh.0 + 1),
            then_block: cases[0].1,
            else_block: default,
            span,
        });
    }
    if let Some(mutation) = mutation {
        if matches!(mutation, Guard | GuardTag) {
            let block = function
                .blocks
                .iter_mut()
                .find(|block| block.id == guard_id)
                .unwrap();
            match mutation {
                Guard => {
                    block.terminator = Some(T::Jump {
                        target: match block.terminator.as_ref().unwrap() {
                            T::Switch { cases, .. } => cases[0].1,
                            _ => unreachable!(),
                        },
                        arguments: vec![],
                        span,
                    })
                }
                GuardTag => {
                    let T::Switch { cases, .. } = block.terminator.as_mut().unwrap() else {
                        unreachable!()
                    };
                    cases[0].0 = 1;
                }
                _ => unreachable!(),
            }
        } else {
            let block = function
                .blocks
                .iter_mut()
                .find(|block| block.id == transfer_id)
                .unwrap();
            let stores = block
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(index, instruction)| {
                    matches!(instruction, I::ArraySet { .. }).then_some(index)
                })
                .collect::<Vec<_>>();
            assert_eq!(stores.len(), 3);
            let call = block
                .instructions
                .iter()
                .position(|instruction| matches!(instruction, I::ClosureCall { .. }))
                .unwrap();
            let read = block
                .instructions
                .iter()
                .position(|instruction| matches!(instruction, I::ArrayGet { .. }))
                .unwrap();
            let [running, consumed] =
                [stores[0], stores[2]].map(|index| match block.instructions[index] {
                    I::ArraySet { new_value, .. } => new_value,
                    _ => unreachable!(),
                });
            match mutation {
                Running | Clear | Consumed => {
                    block.instructions.remove(
                        stores[match mutation {
                            Running => 0,
                            Clear => 1,
                            _ => 2,
                        }],
                    );
                }
                RunningValue | ConsumedValue | StateOrder => {
                    for instruction in &mut block.instructions {
                        if instruction.destination() == Some(running)
                            && matches!(mutation, RunningValue | StateOrder)
                        {
                            let I::Constant { value, .. } = instruction else {
                                panic!("Running literal")
                            };
                            *value = if matches!(mutation, StateOrder) { 2 } else { 3 };
                        }
                        if instruction.destination() == Some(consumed)
                            && matches!(mutation, ConsumedValue | StateOrder)
                        {
                            let I::Constant { value, .. } = instruction else {
                                panic!("Consumed literal")
                            };
                            *value = if matches!(mutation, StateOrder) { 1 } else { 3 };
                        }
                    }
                }
                ClearValue => {
                    let result = block.instructions[read].destination().unwrap();
                    let I::ArraySet { new_value, .. } = &mut block.instructions[stores[1]] else {
                        unreachable!()
                    };
                    *new_value = result;
                }
                ReadOrder => {
                    let mut loaded = block.instructions.remove(read);
                    let I::ArrayGet { index, .. } = &mut loaded else {
                        unreachable!()
                    };
                    *index = status_index;
                    block.instructions.insert(stores[0], loaded);
                }
                Callee | Answer => {
                    let I::ClosureCall {
                        function: callee,
                        arguments,
                        ..
                    } = &mut block.instructions[call]
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
                    let T::Jump { arguments, .. } = block.terminator.as_mut().unwrap() else {
                        unreachable!()
                    };
                    arguments[2] = parameters[2];
                }
                ExtraCall => {
                    let mut extra = block.instructions[call].clone();
                    let ty = function
                        .values
                        .iter()
                        .find(|value| Some(value.id) == extra.destination())
                        .unwrap()
                        .ty;
                    let I::ClosureCall { destination, .. } = &mut extra else {
                        unreachable!()
                    };
                    *destination = fresh;
                    function.values.push(ValueDecl { id: fresh, ty });
                    block.instructions.insert(call + 1, extra);
                }
                ExtraStore => {
                    let mut extra = block.instructions[stores[2]].clone();
                    let I::ArraySet { new_value, .. } = &mut extra else {
                        unreachable!()
                    };
                    *new_value = status_index;
                    block.instructions.insert(stores[2] + 1, extra);
                }
                _ => unreachable!(),
            }
        }
    }
    let result = psrs_backend::mir::verify_module(&module);
    if mutation.is_some() {
        let errors = result.expect_err("malformed owned transfer");
        assert!(
            errors
                .iter()
                .any(|error| error.pass == "P9 MIR owned execution"),
            "{mutation:?}: {errors:?}"
        );
    } else {
        result.expect("equivalent Pending predicate");
    }
}

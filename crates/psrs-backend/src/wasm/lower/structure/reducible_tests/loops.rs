use super::super::tests::{block, count_loops, function, lower_and_validate};
use super::{
    assert_no_dispatcher, branch, constant, decls, execute_cases, execute_trap, jump, primitive,
    require_wasmtime, ret, switch, unreachable,
};
use crate::mir::NumericOp;
use crate::types::{ValueId, ValueType};

#[test]
fn structures_and_executes_a_diamond_inside_a_loop() {
    // B1 is a natural-loop header with loop-carried (i, acc). Its body holds a
    // two-arm diamond that rejoins at B6 before the back edge. Every arm ends
    // by jumping to the shared successor with a block argument.
    let values = decls(&[
        (0, ValueType::I32),
        (1, ValueType::I32),
        (2, ValueType::I32),
        (3, ValueType::I32),
        (4, ValueType::I32),
        (5, ValueType::Boolean),
        (6, ValueType::I32),
        (7, ValueType::I32),
        (8, ValueType::I32),
        (9, ValueType::I32),
        (10, ValueType::I32),
        (11, ValueType::Boolean),
    ]);
    let mir = function(
        "loop_diamond",
        values,
        vec![
            block(
                0,
                Vec::new(),
                vec![constant(3, 1), constant(4, 100), constant(10, 0)],
                jump(1, vec![0, 10]),
            ),
            block(
                1,
                vec![ValueId(1), ValueId(2)],
                vec![primitive(11, NumericOp::I32GtS, 1, 10)],
                branch(11, 2, 5),
            ),
            block(
                2,
                Vec::new(),
                vec![primitive(5, NumericOp::I32Eq, 1, 3)],
                branch(5, 3, 4),
            ),
            block(
                3,
                Vec::new(),
                vec![primitive(6, NumericOp::I32Add, 2, 4)],
                jump(6, vec![6]),
            ),
            block(
                4,
                Vec::new(),
                vec![primitive(7, NumericOp::I32Add, 2, 1)],
                jump(6, vec![7]),
            ),
            block(
                6,
                vec![ValueId(8)],
                vec![primitive(9, NumericOp::I32Sub, 1, 3)],
                jump(1, vec![9, 8]),
            ),
            block(5, Vec::new(), Vec::new(), ret(2)),
        ],
        ValueId(2),
    );

    let (lowered, bytes) = lower_and_validate(&mir);
    assert_eq!(count_loops(&lowered.body), 1);
    assert_no_dispatcher("loop_diamond", &lowered, &mir);
    if require_wasmtime("loop_diamond") {
        execute_cases(
            "loop_diamond",
            &bytes,
            &[("0", "0"), ("1", "100"), ("3", "105")],
        );
    }
}

#[test]
fn structures_and_executes_a_switch_inside_a_loop() {
    // A natural loop whose body ends in a sparse-tag `Switch`. Case 3 adds 30,
    // case 2 adds 20, and the default adds 1; the arms rejoin at B6.
    let values = decls(&[
        (0, ValueType::I32),
        (1, ValueType::I32),
        (2, ValueType::I32),
        (3, ValueType::I32),
        (4, ValueType::I32),
        (5, ValueType::I32),
        (6, ValueType::I32),
        (7, ValueType::I32),
        (8, ValueType::I32),
        (9, ValueType::I32),
        (10, ValueType::I32),
        (11, ValueType::I32),
        (12, ValueType::Boolean),
    ]);
    let mir = function(
        "loop_switch",
        values,
        vec![
            block(
                0,
                Vec::new(),
                vec![
                    constant(3, 1),
                    constant(4, 20),
                    constant(5, 30),
                    constant(6, 0),
                ],
                jump(1, vec![0, 6]),
            ),
            block(
                1,
                vec![ValueId(1), ValueId(2)],
                vec![primitive(12, NumericOp::I32GtS, 1, 6)],
                branch(12, 2, 7),
            ),
            block(
                2,
                Vec::new(),
                Vec::new(),
                switch(1, vec![(3, 3), (2, 4)], 5),
            ),
            block(
                3,
                Vec::new(),
                vec![primitive(7, NumericOp::I32Add, 2, 5)],
                jump(6, vec![7]),
            ),
            block(
                4,
                Vec::new(),
                vec![primitive(8, NumericOp::I32Add, 2, 4)],
                jump(6, vec![8]),
            ),
            block(
                5,
                Vec::new(),
                vec![primitive(9, NumericOp::I32Add, 2, 3)],
                jump(6, vec![9]),
            ),
            block(
                6,
                vec![ValueId(10)],
                vec![primitive(11, NumericOp::I32Sub, 1, 3)],
                jump(1, vec![11, 10]),
            ),
            block(7, Vec::new(), Vec::new(), ret(2)),
        ],
        ValueId(2),
    );

    let (lowered, bytes) = lower_and_validate(&mir);
    assert_eq!(count_loops(&lowered.body), 1);
    assert_no_dispatcher("loop_switch", &lowered, &mir);
    if require_wasmtime("loop_switch") {
        execute_cases(
            "loop_switch",
            &bytes,
            &[("0", "0"), ("3", "51"), ("4", "52")],
        );
    }
}

#[test]
fn structures_and_executes_a_branch_with_an_early_return_inside_a_loop() {
    // One arm of the inner branch returns; the other keeps looping. The arms
    // share no join, so the uniform structurer must branch to their labels.
    let values = decls(&[
        (0, ValueType::I32),
        (1, ValueType::I32),
        (2, ValueType::I32),
        (3, ValueType::I32),
        (4, ValueType::I32),
        (5, ValueType::I32),
        (6, ValueType::I32),
        (7, ValueType::Boolean),
        (8, ValueType::Boolean),
        (9, ValueType::I32),
        (10, ValueType::I32),
    ]);
    let mir = function(
        "loop_early_return",
        values,
        vec![
            block(
                0,
                Vec::new(),
                vec![
                    constant(3, 1),
                    constant(4, 5),
                    constant(5, 999),
                    constant(6, 0),
                ],
                jump(1, vec![0, 6]),
            ),
            block(
                1,
                vec![ValueId(1), ValueId(2)],
                vec![primitive(7, NumericOp::I32GtS, 1, 6)],
                branch(7, 2, 5),
            ),
            block(
                2,
                Vec::new(),
                vec![primitive(8, NumericOp::I32Eq, 1, 4)],
                branch(8, 3, 4),
            ),
            block(3, Vec::new(), Vec::new(), ret(5)),
            block(
                4,
                Vec::new(),
                vec![
                    primitive(9, NumericOp::I32Add, 2, 1),
                    primitive(10, NumericOp::I32Sub, 1, 3),
                ],
                jump(1, vec![10, 9]),
            ),
            block(5, Vec::new(), Vec::new(), ret(2)),
        ],
        ValueId(2),
    );

    let (lowered, bytes) = lower_and_validate(&mir);
    assert_eq!(count_loops(&lowered.body), 1);
    assert_no_dispatcher("loop_early_return", &lowered, &mir);
    if require_wasmtime("loop_early_return") {
        execute_cases(
            "loop_early_return",
            &bytes,
            &[("0", "0"), ("3", "6"), ("5", "999")],
        );
    }
}

#[test]
fn structures_and_executes_a_trapping_arm_inside_a_loop() {
    // A branch arm traps through `Unreachable`; the other continues the loop.
    // The structurer must not emit the trapping block's terminator.
    let values = decls(&[
        (0, ValueType::I32),
        (1, ValueType::I32),
        (2, ValueType::I32),
        (3, ValueType::I32),
        (4, ValueType::I32),
        (5, ValueType::I32),
        (6, ValueType::Boolean),
        (7, ValueType::Boolean),
        (8, ValueType::I32),
        (9, ValueType::I32),
        (10, ValueType::I32),
    ]);
    let mir = function(
        "loop_trap",
        values,
        vec![
            block(
                0,
                Vec::new(),
                vec![constant(3, 1), constant(4, 5), constant(5, 0)],
                jump(1, vec![0, 5]),
            ),
            block(
                1,
                vec![ValueId(1), ValueId(2)],
                vec![primitive(6, NumericOp::I32GtS, 1, 5)],
                branch(6, 2, 5),
            ),
            block(
                2,
                Vec::new(),
                vec![primitive(7, NumericOp::I32Eq, 1, 4)],
                branch(7, 3, 4),
            ),
            block(3, Vec::new(), vec![unreachable(10)], ret(10)),
            block(
                4,
                Vec::new(),
                vec![
                    primitive(8, NumericOp::I32Add, 2, 1),
                    primitive(9, NumericOp::I32Sub, 1, 3),
                ],
                jump(1, vec![9, 8]),
            ),
            block(5, Vec::new(), Vec::new(), ret(2)),
        ],
        ValueId(2),
    );

    let (lowered, bytes) = lower_and_validate(&mir);
    assert_eq!(count_loops(&lowered.body), 1);
    assert_no_dispatcher("loop_trap", &lowered, &mir);
    if require_wasmtime("loop_trap") {
        execute_cases("loop_trap", &bytes, &[("0", "0"), ("3", "6")]);
        execute_trap("loop_trap", &bytes, "5");
    }
}

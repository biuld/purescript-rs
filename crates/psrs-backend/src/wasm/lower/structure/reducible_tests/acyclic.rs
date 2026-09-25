use super::super::tests::{block, count_loops, function, lower_and_validate};
use super::{
    assert_no_dispatcher, branch, constant, decls, execute_cases, jump, primitive,
    require_wasmtime, ret,
};
use crate::mir::NumericOp;
use crate::types::{ValueId, ValueType};

#[test]
fn structures_and_executes_an_acyclic_branch_without_a_common_join() {
    // Both arms return. There is no common join, but that is irrelevant to the
    // unified reducible structurer: it emits each block once inside nested
    // `block`s and branches to the arm labels directly. The join derivation
    // (`common_join`) is an optimization aid, not an emission precondition.
    let values = decls(&[
        (0, ValueType::Boolean),
        (1, ValueType::I32),
        (2, ValueType::I32),
    ]);
    let mir = function(
        "no_join",
        values,
        vec![
            block(0, Vec::new(), Vec::new(), branch(0, 1, 2)),
            block(1, Vec::new(), vec![constant(1, 10)], ret(1)),
            block(2, Vec::new(), vec![constant(2, 20)], ret(2)),
        ],
        ValueId(1),
    );

    let (lowered, bytes) = lower_and_validate(&mir);
    assert_eq!(count_loops(&lowered.body), 0);
    assert_no_dispatcher("no_join", &lowered, &mir);
    if require_wasmtime("no_join") {
        execute_cases("no_join", &bytes, &[("0", "20"), ("1", "10")]);
    }
}

#[test]
fn rejects_an_acyclic_branch_to_a_missing_target() {
    // A branch naming a block that is not part of the function is genuinely
    // invalid MIR and must still be rejected with a diagnostic.
    let values = decls(&[(0, ValueType::Boolean), (1, ValueType::I32)]);
    let mir = function(
        "missing_target",
        values,
        vec![
            block(0, Vec::new(), Vec::new(), branch(0, 1, 9)),
            block(1, Vec::new(), vec![constant(1, 1)], ret(1)),
        ],
        ValueId(1),
    );

    let error = super::super::super::lower_function(
        &mir,
        crate::wasm::TypeIndex(0),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
    )
    .expect_err("a branch to a missing block must not structure");
    assert!(
        error.iter().any(|error| error.message.contains("missing")),
        "{error:?}"
    );
}

#[test]
fn structures_and_executes_a_shared_successor_with_swapped_block_arguments() {
    // Both predecessors jump to one successor. The then-arm swaps the argument
    // order, so the structurer must read every argument before writing any
    // target parameter. B3 computes p * 100 + q.
    let values = decls(&[
        (0, ValueType::Boolean),
        (1, ValueType::I32),
        (2, ValueType::I32),
        (3, ValueType::I32),
        (4, ValueType::I32),
        (5, ValueType::I32),
        (6, ValueType::I32),
        (7, ValueType::I32),
    ]);
    let mir = function(
        "swapped_join",
        values,
        vec![
            block(
                0,
                Vec::new(),
                vec![constant(1, 10), constant(2, 20), constant(3, 100)],
                branch(0, 1, 2),
            ),
            block(1, Vec::new(), Vec::new(), jump(3, vec![2, 1])),
            block(2, Vec::new(), Vec::new(), jump(3, vec![1, 2])),
            block(
                3,
                vec![ValueId(4), ValueId(5)],
                vec![
                    primitive(6, NumericOp::I32Mul, 4, 3),
                    primitive(7, NumericOp::I32Add, 6, 5),
                ],
                ret(7),
            ),
        ],
        ValueId(7),
    );

    let (lowered, bytes) = lower_and_validate(&mir);
    assert_eq!(count_loops(&lowered.body), 0);
    assert_no_dispatcher("swapped_join", &lowered, &mir);
    if require_wasmtime("swapped_join") {
        execute_cases("swapped_join", &bytes, &[("0", "1020"), ("1", "2010")]);
    }
}

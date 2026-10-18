//! EH-10 fixtures for the try unit's interaction with natural loops: a try
//! re-armed across loop iterations ("try inside loop"), and a loop kept
//! inside the protected region.

use super::*;

// ------------------------------------------------------------ try in a loop

/// Every iteration re-enters the try block through the loop header, so the
/// `try_table` is armed again for each iteration; the catch fires on the
/// third one.
#[test]
fn re_arms_a_try_table_across_loop_iterations() {
    let blocks = vec![
        block(
            0,
            vec![],
            vec![constant(9, 0, at(30))],
            jump(1, vec![9], at(30)),
        ),
        block(
            1,
            vec![1],
            vec![
                constant(2, 3, at(31)),
                primitive(3, NumericOp::I32LtS, 1, 2, at(31)),
            ],
            Terminator::Branch {
                condition: ValueId(3),
                then_block: BlockId(2),
                else_block: BlockId(8),
                span: at(31),
            },
        ),
        block(
            2,
            vec![],
            vec![],
            Terminator::Try {
                protected: BlockId(3),
                catches: Vec::new(),
                catch_all: Some(BlockId(6)),
                span: at(32),
            },
        ),
        block(
            3,
            vec![],
            vec![
                constant(4, 2, at(33)),
                primitive(5, NumericOp::I32Eq, 1, 4, at(33)),
            ],
            Terminator::Branch {
                condition: ValueId(5),
                then_block: BlockId(4),
                else_block: BlockId(5),
                span: at(33),
            },
        ),
        block(
            4,
            vec![],
            vec![],
            Terminator::Throw {
                tag: TagId(0),
                arguments: Vec::new(),
                handler: Some(BlockId(6)),
                span: at(34),
            },
        ),
        block(
            5,
            vec![],
            vec![
                constant(6, 1, at(35)),
                primitive(7, NumericOp::I32Add, 1, 6, at(35)),
            ],
            jump(1, vec![7], at(35)),
        ),
        block(
            6,
            vec![],
            vec![constant(8, 99, at(36))],
            jump(9, vec![8], at(36)),
        ),
        block(
            8,
            vec![],
            vec![],
            Terminator::Return {
                value: ValueId(1),
                span: at(37),
            },
        ),
        block(
            9,
            vec![0],
            vec![],
            Terminator::Return {
                value: ValueId(0),
                span: at(38),
            },
        ),
    ];
    let values: Vec<(u32, ValueType)> = (0..=9u32)
        .map(|id| {
            (
                id,
                if matches!(id, 3 | 5) {
                    ValueType::Boolean
                } else {
                    ValueType::I32
                },
            )
        })
        .collect();
    let wat = wat(
        function(blocks.clone(), &values, vec![], 0),
        vec![tag(0, Vec::new())],
    );
    assert_eq!(wat.matches("try_table").count(), 1, "{wat}");
    let loop_start = wat.find("loop").expect("the loop label");
    let try_table = wat.find("try_table").expect("the armed region");
    assert!(
        loop_start < try_table,
        "the try_table must sit inside the loop: {wat}"
    );

    let mir = module(
        function(blocks, &values, vec![], 0),
        vec![tag(0, Vec::new())],
    );
    let Some(output) = run(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(99),
        "the re-armed region must catch the third iteration's throw: {output:?}"
    );
}

// ------------------------------------------------- loop inside the region

/// The loop lives inside the protected region: the back edge targets the
/// protected entry (the loop header), and the `try_table` is armed across
/// every iteration; the throw fires mid-loop and the handler reports it.
#[test]
fn keeps_a_loop_inside_the_protected_region() {
    let blocks = vec![
        block(
            0,
            vec![],
            vec![],
            Terminator::Try {
                // The protected entry is a parameterless pre-header; the loop
                // header (with its carried parameter) sits inside the region.
                protected: BlockId(10),
                catches: Vec::new(),
                catch_all: Some(BlockId(6)),
                span: at(40),
            },
        ),
        block(
            10,
            vec![],
            vec![constant(7, 0, at(47))],
            jump(1, vec![7], at(47)),
        ),
        block(
            1,
            vec![1],
            vec![
                constant(2, 2, at(41)),
                primitive(3, NumericOp::I32Eq, 1, 2, at(41)),
            ],
            Terminator::Branch {
                condition: ValueId(3),
                then_block: BlockId(4),
                else_block: BlockId(2),
                span: at(41),
            },
        ),
        block(
            2,
            vec![],
            vec![
                constant(4, 1, at(42)),
                primitive(5, NumericOp::I32Add, 1, 4, at(42)),
            ],
            jump(1, vec![5], at(42)),
        ),
        block(
            4,
            vec![],
            vec![],
            Terminator::Throw {
                tag: TagId(0),
                arguments: Vec::new(),
                handler: Some(BlockId(6)),
                span: at(43),
            },
        ),
        block(
            6,
            vec![],
            vec![constant(6, 42, at(44))],
            jump(9, vec![6], at(44)),
        ),
        block(
            9,
            vec![0],
            vec![],
            Terminator::Return {
                value: ValueId(0),
                span: at(45),
            },
        ),
    ];
    let values: Vec<(u32, ValueType)> = (0..=7u32)
        .map(|id| {
            (
                id,
                if id == 3 {
                    ValueType::Boolean
                } else {
                    ValueType::I32
                },
            )
        })
        .collect();
    let wat = wat(
        function(blocks.clone(), &values, vec![], 0),
        vec![tag(0, Vec::new())],
    );
    assert_eq!(wat.matches("try_table").count(), 1, "{wat}");
    let try_table = wat.find("try_table").expect("the armed region");
    let loop_start = wat.find("loop").expect("the loop label");
    assert!(
        try_table < loop_start,
        "the loop must sit inside the protected region: {wat}"
    );

    let mir = module(
        function(blocks, &values, vec![], 0),
        vec![tag(0, Vec::new())],
    );
    let Some(output) = run(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "the handler must catch the throw fired mid-loop: {output:?}"
    );
}

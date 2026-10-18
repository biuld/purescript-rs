//! The budget's runtime fixtures (EH-13, EH-14): deep tail recursion must not
//! fire the budget, and a throw/catch loop followed by deep non-tail
//! recursion must reach the budget point — the watermark contract's leak
//! fixture. Shared fixture builders live in the parent `budget` module. The
//! EH-15 coordination fixture (the CC `checkThrows` discharge) lives in
//! `coordination`.

use super::*;

fn run(mir: &crate::mir::Module) -> Option<std::process::Output> {
    if !super::execution::require_engine() {
        return None;
    }
    super::execution::run_output(mir, enabled_target())
}

fn boolean(value: u32, v: i32, span: TextRange) -> Instruction {
    Instruction::Constant {
        destination: ValueId(value),
        value: v,
        span,
    }
}

/// `main` of the leak fixture: a loop of guarded `thrower` calls — each
/// iteration throws and catches, which leaks the unwound frame's weight when
/// the watermark restore is missing — then a *shallow* guarded `rec(100)`
/// that must return (a leak fires the budget here, exit 2), then the deep
/// guarded `rec(DEEP)` whose catch is the expected exit 1.
fn leak_main() -> Function {
    let blocks = vec![
        // The throw/catch loop.
        block(
            0,
            vec![],
            vec![constant(20, 0, at(80))],
            jump(1, vec![20], at(80)),
        ),
        block(
            1,
            vec![0],
            vec![
                constant(1, leak_iterations() as i32, at(80)),
                primitive(2, NumericOp::I32LtS, 0, 1, at(80)),
            ],
            Terminator::Branch {
                condition: ValueId(2),
                then_block: BlockId(2),
                else_block: BlockId(6),
                span: at(80),
            },
        ),
        block(
            2,
            vec![],
            vec![],
            Terminator::Try {
                protected: BlockId(3),
                catches: Vec::new(),
                catch_all: Some(BlockId(4)),
                span: at(80),
            },
        ),
        block(
            3,
            vec![],
            vec![call(3, THROWER, vec![], at(80))],
            jump(5, vec![0], at(80)),
        ),
        block(4, vec![], vec![], jump(5, vec![0], at(84))),
        block(
            5,
            vec![19],
            vec![
                constant(4, 1, at(84)),
                primitive(5, NumericOp::I32Add, 19, 4, at(84)),
            ],
            jump(1, vec![5], at(84)),
        ),
        // The shallow guarded recursion: must return normally (exit 2 means
        // the counter leaked past the budget during the loop).
        block(
            6,
            vec![],
            vec![],
            Terminator::Try {
                protected: BlockId(7),
                catches: Vec::new(),
                catch_all: Some(BlockId(8)),
                span: at(84),
            },
        ),
        block(
            7,
            vec![],
            vec![
                constant(6, 100, at(84)),
                call(7, REC, vec![6], at(84)),
                boolean(8, 0, at(84)),
            ],
            jump(12, vec![8], at(84)),
        ),
        block(
            8,
            vec![],
            vec![boolean(9, 1, at(88))],
            jump(12, vec![9], at(88)),
        ),
        // The deep guarded recursion: must fire the budget (exit 1); a
        // program that never fires it exits 3, and an engine trap is a
        // different failure entirely.
        block(
            9,
            vec![],
            vec![],
            Terminator::Try {
                protected: BlockId(10),
                catches: Vec::new(),
                catch_all: Some(BlockId(11)),
                span: at(88),
            },
        ),
        block(
            10,
            vec![],
            vec![
                constant(11, 100_000, at(88)),
                call(12, REC, vec![11], at(88)),
                boolean(13, 0, at(88)),
            ],
            jump(15, vec![13], at(88)),
        ),
        block(
            11,
            vec![],
            vec![boolean(14, 1, at(91))],
            jump(15, vec![14], at(91)),
        ),
        block(
            12,
            vec![17],
            vec![],
            Terminator::Branch {
                condition: ValueId(17),
                then_block: BlockId(16),
                else_block: BlockId(9),
                span: at(91),
            },
        ),
        block(
            16,
            vec![],
            vec![constant(10, 2, at(96))],
            return_of(10, at(96)),
        ),
        block(
            15,
            vec![18],
            vec![],
            Terminator::Branch {
                condition: ValueId(18),
                then_block: BlockId(17),
                else_block: BlockId(19),
                span: at(96),
            },
        ),
        block(
            17,
            vec![],
            vec![constant(15, 1, at(97))],
            return_of(15, at(97)),
        ),
        block(
            19,
            vec![],
            vec![constant(16, 3, at(99))],
            return_of(16, at(99)),
        ),
    ];
    Function {
        id: FunctionId(MAIN as u32),
        symbol: symbol(MAIN),
        name: "main".into(),
        parameters: Vec::new(),
        values: (0..=20u32)
            .map(|id| ValueDecl {
                id: ValueId(id),
                ty: if matches!(id, 2 | 8 | 9 | 13 | 14 | 17 | 18) {
                    ValueType::Boolean
                } else {
                    ValueType::I32
                },
            })
            .collect(),
        entry: BlockId(0),
        blocks,
        result: ValueId(0),
        result_type: ValueType::I32,
        frame_weight: budget::frame_weight(20),
        span: at(905),
    }
}

fn return_of(value: u32, span: TextRange) -> Terminator {
    Terminator::Return {
        value: ValueId(value),
        span,
    }
}

/// Enough throw/catch iterations that a *missing* watermark restore — which
/// leaks the thrower's frame weight per iteration — pushes the counter past
/// `STACK_BUDGET`, firing the budget on the fixture's shallow call; with the
/// restore, the counter is settled each iteration and the shallow call runs.
fn leak_iterations() -> usize {
    let leak_per_iteration = budget::frame_weight(1) as usize;
    4 * (budget::STACK_BUDGET as usize / leak_per_iteration + 1)
}

/// EH-13: deep tail recursion still exits 0 — the `return_call` decrements
/// keep the counter bounded, so the budget never fires and no false throw
/// replaces the normal result.
#[test]
fn deep_tail_recursion_exits_0_without_a_false_throw() {
    let span = at(85);
    let main = Function {
        id: FunctionId(MAIN as u32),
        symbol: symbol(MAIN),
        name: "main".into(),
        parameters: Vec::new(),
        values: vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(1),
                ty: ValueType::I32,
            },
        ],
        entry: BlockId(0),
        blocks: vec![block(
            0,
            vec![],
            vec![constant(0, 1_000_000, span), call(1, EVEN, vec![0], span)],
            return_of(1, span),
        )],
        result: ValueId(1),
        result_type: ValueType::I32,
        frame_weight: budget::frame_weight(2),
        span: at(906),
    };
    let mir = module(
        vec![main, rec_function(), even_function(), odd_function()],
        Vec::new(),
        MAIN,
    );
    let Some(output) = run(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "deep tail recursion must return normally: {output:?}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("call stack exhausted"),
        "the budget must not misfire on tail recursion: {output:?}"
    );
}

pub(super) const EVEN: usize = 2;
pub(super) const ODD: usize = 3;

/// `parity(n) = if n == 0 then base else parity(n - 1)` — a `return_call`, so
/// the frames replace each other and the counter stays bounded.
fn tail_function(index: usize, name: &str, other: usize, base: i32) -> Function {
    let span = at(88);
    let blocks = vec![
        block(
            0,
            vec![],
            vec![
                constant(1, 0, span),
                primitive(2, NumericOp::I32Eq, 0, 1, span),
            ],
            Terminator::Branch {
                condition: ValueId(2),
                then_block: BlockId(1),
                else_block: BlockId(2),
                span,
            },
        ),
        block(1, vec![], vec![constant(3, base, span)], return_of(3, span)),
        block(
            2,
            vec![],
            vec![
                constant(4, 1, span),
                primitive(5, NumericOp::I32Sub, 0, 4, span),
            ],
            Terminator::ReturnCall {
                contract: None,
                function: symbol(other),
                arguments: vec![ValueId(5)],
                span,
            },
        ),
    ];
    Function {
        id: FunctionId(index as u32),
        symbol: symbol(index),
        name: name.into(),
        parameters: vec![ValueId(0)],
        values: (0..=5u32)
            .map(|id| ValueDecl {
                id: ValueId(id),
                ty: if id == 2 {
                    ValueType::Boolean
                } else {
                    ValueType::I32
                },
            })
            .collect(),
        entry: BlockId(0),
        blocks,
        result: ValueId(3),
        result_type: ValueType::I32,
        frame_weight: budget::frame_weight(6),
        span: at(907),
    }
}

fn even_function() -> Function {
    tail_function(EVEN, "even", ODD, 0)
}

fn odd_function() -> Function {
    tail_function(ODD, "odd", EVEN, 0)
}

/// EH-14: the leak fixture. Without the watermark restore, the loop leaks the
/// thrower's frame weight per iteration past `STACK_BUDGET`, the shallow
/// guard fires, and the fixture exits 2; with the restore the shallow guard
/// returns and the deep guard reaches the budget point, exiting 1.
#[test]
fn a_throw_catch_loop_does_not_leak_the_depth_counter() {
    let main = leak_main();
    let mir = module(
        vec![main, rec_function(), thrower_function()],
        vec![exhaustion_tag()],
        MAIN,
    );
    let Some(output) = run(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(1),
        "the deep recursion must reach the budget point after the loop: {output:?}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("call stack exhausted"),
        "the budget must fire before the engine's trap: {output:?}"
    );
}

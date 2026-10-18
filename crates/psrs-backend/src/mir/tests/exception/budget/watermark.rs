//! EH-14: every emitted `try_table` is immediately preceded by the
//! watermark save into the try region's fresh `i32` local, and its catch
//! path — the code right after the enclosing unwind block's `end` — begins
//! with the restore. Shared builders live in the parent `budget` module.

use super::*;

// ------------------------------------------------------------------ EH-14

/// Every `try_table` in the lowered module is immediately preceded by the
/// watermark save (`global.get depth; local.set saved`) and its catch path —
/// the code right after the enclosing unwind block's `end` — begins with the
/// restore (`local.get saved; global.set depth`).
#[test]
fn every_try_table_carries_the_watermark_save_and_restore() {
    // One nested-try function: outer try over an inner try, the throw caught
    // by the inner handler — two try regions, two watermark locals.
    fn try_blocks(protected: BlockId, catch: BlockId) -> Terminator {
        Terminator::Try {
            protected,
            catches: Vec::new(),
            catch_all: Some(catch),
            span: at(10),
        }
    }
    let main = function(
        MAIN,
        "main",
        vec![],
        vec![
            (0, ValueType::I32),
            (1, ValueType::I32),
            (2, ValueType::I32),
        ],
        0,
        vec![
            block(0, vec![], vec![], try_blocks(BlockId(1), BlockId(8))),
            block(1, vec![], vec![], try_blocks(BlockId(2), BlockId(3))),
            block(
                2,
                vec![],
                vec![],
                Terminator::Throw {
                    tag: TagId(0),
                    arguments: Vec::new(),
                    handler: Some(BlockId(3)),
                    span: at(12),
                },
            ),
            block(3, vec![], vec![], Terminator::Trap { span: at(13) }),
            block(
                8,
                vec![],
                vec![constant(1, 20, at(14))],
                jump(9, vec![1], at(14)),
            ),
            block(
                9,
                vec![(0)],
                vec![],
                Terminator::Return {
                    value: ValueId(0),
                    span: at(15),
                },
            ),
        ],
    );
    let mir = module(vec![main], vec![exhaustion_tag()], MAIN);
    let wasm = lower(&mir, enabled_target());
    let wat = wat(&wasm);

    // Two try regions in the function, plus the entry's synthesized
    // `catch_all` region: three `try_table`s in the module.
    assert_eq!(wat.matches("try_table").count(), 3, "{wat}");
    // The body walk checks the save/restore adjacency precisely on the thin
    // IR, per try region, in emission order.
    let function = &wasm.functions[0];
    let saves = count_watermark_saves(&function.body);
    let restores = count_watermark_restores(&function.body);
    assert_eq!(saves, 2, "one save per try region: {wat}");
    assert_eq!(restores, 2, "one restore per catch path: {wat}");

    // The entry's synthesized region carries its own save (at entry) and its
    // catch path begins with the restore.
    let entry = wasm.entry.as_ref().expect("the run entry");
    assert_eq!(count_watermark_saves(&entry.body), 1, "{wat}");
    assert_eq!(count_watermark_restores(&entry.body), 1, "{wat}");
}

fn count_watermark_saves(body: &[crate::wasm::Op]) -> usize {
    use wasm_encoder::Instruction;
    body.windows(2)
        .filter(|window| {
            matches!(
                (&window[0], &window[1]),
                (
                    crate::wasm::Op::Leaf(Instruction::GlobalGet(_)),
                    crate::wasm::Op::Leaf(Instruction::LocalSet(_))
                )
            )
        })
        .count()
        + body
            .iter()
            .map(|op| match op {
                crate::wasm::Op::Block { body, .. } | crate::wasm::Op::Loop { body, .. } => {
                    count_watermark_saves(body)
                }
                crate::wasm::Op::Try { body, .. } => count_watermark_saves(body),
                crate::wasm::Op::If {
                    then_body,
                    else_body,
                    ..
                } => count_watermark_saves(then_body) + count_watermark_saves(else_body),
                _ => 0,
            })
            .sum::<usize>()
}

fn count_watermark_restores(body: &[crate::wasm::Op]) -> usize {
    use wasm_encoder::Instruction;
    body.windows(2)
        .filter(|window| {
            matches!(
                (&window[0], &window[1]),
                (
                    crate::wasm::Op::Leaf(Instruction::LocalGet(_)),
                    crate::wasm::Op::Leaf(Instruction::GlobalSet(_))
                )
            )
        })
        .count()
        + body
            .iter()
            .map(|op| match op {
                crate::wasm::Op::Block { body, .. } | crate::wasm::Op::Loop { body, .. } => {
                    count_watermark_restores(body)
                }
                crate::wasm::Op::Try { body, .. } => count_watermark_restores(body),
                crate::wasm::Op::If {
                    then_body,
                    else_body,
                    ..
                } => count_watermark_restores(then_body) + count_watermark_restores(else_body),
                _ => 0,
            })
            .sum::<usize>()
}

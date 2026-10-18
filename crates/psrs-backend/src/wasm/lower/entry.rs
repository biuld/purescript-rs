//! The synthesized `run` entry: the WASI command export that calls `main` and
//! exits through `exit-with-code`. When the target enables `exceptions`, the
//! entry additionally wraps the call in a `try_table (catch_all)`: an
//! exception no guest handler observes — the stack-exhaustion throw among
//! them — restores the depth counter to the value saved at entry and leaves
//! the artifact through the deterministic exit code 1 (design "Budget
//! emission").

use super::budget::Budget;
use super::{Entry, FunctionIndex, Op, TypeIndex};
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};

pub(super) fn synthesize(
    main_index: FunctionIndex,
    exit_index: Option<FunctionIndex>,
    entry_type: TypeIndex,
    budget: Option<&Budget>,
    span: TextRange,
) -> Entry {
    let mut body: Vec<Op> = Vec::new();
    let mut locals: Vec<ValType> = Vec::new();
    match budget {
        None => {
            body.push(Op::Leaf(Instruction::Call(main_index.0)));
            if let Some(exit_index) = exit_index {
                body.push(Op::Leaf(Instruction::Call(exit_index.0)));
                body.push(Op::Leaf(Instruction::I32Const(0)));
            }
        }
        Some(budget) => {
            // The watermark save at entry: the handler restores the counter
            // to this value, settling `main`'s own prologue increment, which
            // never reaches its epilogue on the throw path.
            let saved = locals.len() as u32;
            locals.push(ValType::I32);
            body.push(Op::Leaf(Instruction::GlobalGet(budget.depth)));
            body.push(Op::Leaf(Instruction::LocalSet(saved)));

            // The protected call: `main`, then the exit with `main`'s code,
            // then the entry's own `0` result — handed back with `return`,
            // because the `try_table`'s empty block type carries no values.
            let mut protected = vec![Op::Leaf(Instruction::Call(main_index.0))];
            if let Some(exit_index) = exit_index {
                protected.push(Op::Leaf(Instruction::Call(exit_index.0)));
                protected.push(Op::Leaf(Instruction::I32Const(0)));
            }
            protected.push(Op::Leaf(Instruction::Return));

            body.push(Op::Block {
                // The catch target: an empty-result block, so the `catch_all`
                // clause can deliver nothing to it.
                result: None,
                span,
                body: vec![
                    Op::Try {
                        body: protected,
                        clauses: vec![super::CatchClause::CatchAll { label: 0 }],
                        span,
                    },
                    // The protected body always returns, so the `try_table`
                    // cannot complete normally.
                    Op::Leaf(Instruction::Unreachable),
                ],
            });
            // The catch path: restore the counter FIRST — `main`'s frames
            // unwound without their epilogues — then leave with exit code 1.
            body.push(Op::Leaf(Instruction::LocalGet(saved)));
            body.push(Op::Leaf(Instruction::GlobalSet(budget.depth)));
            body.push(Op::Leaf(Instruction::I32Const(1)));
            if let Some(exit_index) = exit_index {
                body.push(Op::Leaf(Instruction::Call(exit_index.0)));
                // `exit-with-code` never returns, but the entry's result type
                // still needs its `i32` on the validator's stack.
                body.push(Op::Leaf(Instruction::Unreachable));
            }
        }
    }
    Entry {
        type_index: entry_type,
        locals,
        body,
    }
}

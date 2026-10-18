//! EH-07 tail-call marking inside protected try regions.

use super::{at, block, simple_function, terminator};
use crate::capability::TargetCapabilities;
use crate::mir::lower::tail::rewrite;
use crate::mir::{BlockId, Instruction, Terminator, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};

#[test]
fn mark_tail_keeps_a_protected_region_call_an_ordinary_call() {
    let callee = SymbolId::new(ModuleId(0), 1);
    let mut function = simple_function(
        vec![
            block(
                0,
                vec![],
                vec![],
                Terminator::Try {
                    protected: BlockId(1),
                    catches: vec![],
                    catch_all: Some(BlockId(2)),
                    span: at(10),
                },
            ),
            // The protected block's call is forwarded to the join.
            block(
                1,
                vec![],
                vec![Instruction::Call {
                    contract: None,
                    destination: ValueId(1),
                    function: callee,
                    arguments: vec![],
                    span: at(20),
                }],
                Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(1)],
                    span: at(21),
                },
            ),
            // The handler's call is not protected.
            block(
                2,
                vec![],
                vec![Instruction::Call {
                    contract: None,
                    destination: ValueId(2),
                    function: callee,
                    arguments: vec![],
                    span: at(22),
                }],
                Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(2)],
                    span: at(23),
                },
            ),
            block(
                3,
                vec![4],
                vec![],
                Terminator::Return {
                    value: ValueId(4),
                    span: at(24),
                },
            ),
        ],
        vec![
            (1, ValueType::I32),
            (2, ValueType::I32),
            (4, ValueType::I32),
        ],
    );
    let target = TargetCapabilities {
        tail_call: true,
        exceptions: true,
        ..TargetCapabilities::default()
    };
    rewrite(&mut function, target).expect("tail marking succeeds");

    let protected = &function.blocks[1];
    assert!(
        protected
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::Call { .. })),
        "the protected call stays an instruction"
    );
    assert!(
        matches!(terminator(protected), Terminator::Jump { .. }),
        "the protected block keeps its ordinary return path"
    );

    let handler = &function.blocks[2];
    assert!(
        !handler
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::Call { .. })),
        "the unprotected call is consumed by the tail terminator"
    );
    assert!(
        matches!(
            terminator(handler),
            Terminator::ReturnCall { function, .. } if *function == callee
        ),
        "the unprotected call becomes a forced tail call"
    );
}

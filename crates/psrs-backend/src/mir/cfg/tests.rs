use super::*;
use crate::mir::Terminator;
use crate::types::ValueId;
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn block(id: u32, parameters: Vec<u32>, terminator: Terminator) -> BasicBlock {
    BasicBlock {
        id: BlockId(id),
        parameters: parameters.into_iter().map(ValueId).collect(),
        instructions: Vec::new(),
        terminator: Some(terminator),
    }
}

fn branch(then_block: u32, else_block: u32) -> Terminator {
    Terminator::Branch {
        condition: ValueId(0),
        then_block: BlockId(then_block),
        else_block: BlockId(else_block),
        span: span(),
    }
}

fn jump(target: u32) -> Terminator {
    Terminator::Jump {
        target: BlockId(target),
        arguments: Vec::new(),
        span: span(),
    }
}

fn return_empty() -> Terminator {
    Terminator::Return {
        value: ValueId(0),
        span: span(),
    }
}

#[test]
fn derives_a_diamond_join_from_the_edges() {
    let blocks = vec![
        block(0, vec![], branch(1, 2)),
        block(1, vec![], jump(3)),
        block(2, vec![], jump(3)),
        block(3, vec![7], return_empty()),
    ];
    assert_eq!(
        common_join(&blocks, &[BlockId(1), BlockId(2)]),
        Some(BlockId(3))
    );
}

#[test]
fn derives_the_nearest_common_join() {
    // B0 -> {B1, B2}; B1 -> {B3, B4} -> B5; B2 -> B6; B5 -> B6.
    // The inner join B5 is not reachable from B2, so the outer join is B6.
    let blocks = vec![
        block(0, vec![], branch(1, 2)),
        block(1, vec![], branch(3, 4)),
        block(2, vec![], jump(6)),
        block(3, vec![], jump(5)),
        block(4, vec![], jump(5)),
        block(5, vec![8], jump(6)),
        block(6, vec![9], return_empty()),
    ];
    assert_eq!(
        common_join(&blocks, &[BlockId(3), BlockId(4)]),
        Some(BlockId(5))
    );
    assert_eq!(
        common_join(&blocks, &[BlockId(1), BlockId(2)]),
        Some(BlockId(6))
    );
}

#[test]
fn reports_no_join_when_arms_terminate_independently() {
    let blocks = vec![
        block(0, vec![], branch(1, 2)),
        block(1, vec![], return_empty()),
        block(2, vec![], return_empty()),
    ];
    assert_eq!(common_join(&blocks, &[BlockId(1), BlockId(2)]), None);
}

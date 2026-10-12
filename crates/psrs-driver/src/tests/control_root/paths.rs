//! Cleanup is checked on all normal paths, including multiple physical closes.
use super::{R, SOURCE, binding};
use psrs_backend::cc::ValueId;
use psrs_backend::mir::{BasicBlock, BlockId, Instruction as I, Terminator as T};
use psrs_backend::types::{ValueDecl, ValueType};
use psrs_hir::Intrinsic;

fn branching_cleanup(missing: bool, trap: bool) {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let close = binding(&stages.cc.representations, R::Close).1;
    let table = &stages.cc.representations;
    let function = stages.mir.functions.iter_mut().find(|function| function.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(instruction,
        I::ControlEvidence { contract, .. } if table.control_contracts[contract.0 as usize].operation() == Intrinsic::ControlRun))).unwrap();
    let block_index = function.blocks.iter().position(|block| block.instructions.iter().any(|instruction| matches!(instruction, I::Call { contract: Some(id), .. } if *id == close))).unwrap();
    let close_index = function.blocks[block_index]
        .instructions
        .iter()
        .position(
            |instruction| matches!(instruction, I::Call { contract: Some(id), .. } if *id == close),
        )
        .unwrap();
    let close_instruction = function.blocks[block_index]
        .instructions
        .remove(close_index);
    let suffix = function.blocks[block_index]
        .instructions
        .split_off(close_index);
    let terminator = function.blocks[block_index].terminator.take();
    let condition = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    let alias = ValueId(condition.0 + 1);
    let I::Call {
        destination,
        arguments,
        ..
    } = &close_instruction
    else {
        unreachable!()
    };
    let ty = function
        .values
        .iter()
        .find(|value| value.id == *destination)
        .unwrap()
        .ty;
    function.values.extend([
        ValueDecl {
            id: condition,
            ty: ValueType::Boolean,
        },
        ValueDecl { id: alias, ty },
    ]);
    let mut other = close_instruction.clone();
    if missing || trap {
        other = I::Copy {
            destination: alias,
            value: arguments[0],
            span: function.span,
        };
    } else {
        let I::Call { destination, .. } = &mut other else {
            unreachable!()
        };
        *destination = alias;
    }
    let then_block = BlockId(
        function
            .blocks
            .iter()
            .map(|block| block.id.0)
            .max()
            .unwrap()
            + 1,
    );
    let else_block = BlockId(then_block.0 + 1);
    let join = BlockId(then_block.0 + 2);
    let jump = || {
        Some(T::Jump {
            target: join,
            arguments: vec![],
            span: function.span,
        })
    };
    function.blocks[block_index].instructions.push(I::Constant {
        destination: condition,
        value: 1,
        span: function.span,
    });
    function.blocks[block_index].terminator = Some(T::Branch {
        condition,
        then_block,
        else_block,
        span: function.span,
    });
    function.blocks.extend([
        BasicBlock {
            id: then_block,
            parameters: vec![],
            instructions: vec![close_instruction],
            terminator: jump(),
        },
        BasicBlock {
            id: else_block,
            parameters: vec![],
            instructions: vec![other],
            terminator: if trap {
                Some(T::Trap {
                    span: function.span,
                })
            } else {
                jump()
            },
        },
        BasicBlock {
            id: join,
            parameters: vec![],
            instructions: suffix,
            terminator,
        },
    ]);
    if missing {
        let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("close its root after execution")),
            "{errors:?}"
        );
    } else {
        psrs_backend::mir::verify_module(&stages.mir).unwrap();
    }
}

#[test]
fn root_cleanup_can_be_distributed_across_normal_branches() {
    branching_cleanup(false, false);
}
#[test]
fn root_cleanup_cannot_be_skipped_on_a_normal_branch() {
    branching_cleanup(true, false);
}
#[test]
fn root_normal_cleanup_proof_excludes_trapping_paths() {
    branching_cleanup(false, true);
}

#[test]
fn physical_root_and_close_accept_reference_copies() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let close = binding(&stages.cc.representations, R::Close).1;
    let table = &stages.cc.representations;
    let function = stages.mir.functions.iter_mut().find(|function| function.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(instruction,
        I::ControlEvidence { contract, .. } if table.control_contracts[contract.0 as usize].operation() == Intrinsic::ControlRun))).unwrap();
    let alias = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    let mut changed = false;
    for block in &mut function.blocks {
        if let Some(index) = block.instructions.iter().position(
            |instruction| matches!(instruction, I::Call { contract: Some(id), .. } if *id == close),
        ) {
            let I::Call { arguments, .. } = &mut block.instructions[index] else {
                unreachable!()
            };
            let value = arguments[0];
            let ty = function
                .values
                .iter()
                .find(|declaration| declaration.id == value)
                .unwrap()
                .ty;
            function.values.push(ValueDecl { id: alias, ty });
            arguments[0] = alias;
            block.instructions.insert(
                index,
                I::Copy {
                    destination: alias,
                    value,
                    span: function.span,
                },
            );
            changed = true;
            break;
        }
    }
    assert!(changed);
    psrs_backend::mir::verify_module(&stages.mir).unwrap();
}

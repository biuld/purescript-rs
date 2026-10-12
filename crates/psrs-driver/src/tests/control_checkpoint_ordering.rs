//! A correct checkpoint check must dominate the start of replay.
use super::control_checkpoints::{SOURCE, binding, mir_error};
use psrs_backend::cc::ControlHelperRole as R;
use psrs_backend::mir::Instruction as I;

#[test]
fn mir_rejects_a_correct_checkpoint_check_after_replay_has_started() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let executor = binding(&stages.cc.representations, R::Executor).0;
    let check = binding(&stages.cc.representations, R::Check).1;
    let clone = binding(&stages.cc.representations, R::CloneSequence).1;
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let source = function
        .blocks
        .iter()
        .enumerate()
        .find_map(|(block, body)| {
            body.instructions.iter().position(|instruction|
            matches!(instruction, I::Call { contract: Some(id), .. } if *id == check))
            .map(|index| (block, index))
        })
        .unwrap();
    let instruction = function.blocks[source.0].instructions.remove(source.1);
    let target = function
        .blocks
        .iter()
        .enumerate()
        .find_map(|(block, body)| {
            body.instructions.iter().position(|instruction|
            matches!(instruction, I::Call { contract: Some(id), .. } if *id == clone))
            .map(|index| (block, index))
        })
        .unwrap();
    function.blocks[target.0]
        .instructions
        .insert(target.1 + 1, instruction);
    mir_error(
        &stages.mir,
        "check its saved checkpoint before construction or replay",
    );
}

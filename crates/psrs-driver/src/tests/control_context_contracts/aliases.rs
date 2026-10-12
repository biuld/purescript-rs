//! A dispatch join retains identity through an ordinary SSA copy.
use super::{R, SOURCE, binding};
use psrs_backend::cc::ValueId;
use psrs_backend::mir::{Instruction as I, Terminator as T};

#[test]
fn dispatch_parent_join_accepts_reference_identity_copies() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let executor = binding(&stages.cc.representations, R::Executor).0;
    let clone = binding(&stages.cc.representations, R::CloneSequence).1;
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let parent = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            I::Call {
                contract: Some(id),
                arguments,
                ..
            } if *id == clone => Some(arguments[4]),
            _ => None,
        })
        .unwrap();
    let (target, index) = function
        .blocks
        .iter()
        .find_map(|block| {
            block
                .parameters
                .iter()
                .position(|parameter| *parameter == parent)
                .map(|index| (block.id, index))
        })
        .unwrap();
    let source_block = function.blocks.iter().position(|block| matches!(&block.terminator, Some(T::Jump { target: next, .. }) if *next == target)).unwrap();
    let Some(T::Jump { arguments, .. }) = &function.blocks[source_block].terminator else {
        unreachable!()
    };
    let value = arguments[index];
    let ty = function
        .values
        .iter()
        .find(|declaration| declaration.id == value)
        .unwrap()
        .ty;
    let alias = ValueId(
        function
            .values
            .iter()
            .map(|declaration| declaration.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function
        .values
        .push(psrs_backend::types::ValueDecl { id: alias, ty });
    function.blocks[source_block].instructions.push(I::Copy {
        destination: alias,
        value,
        span: function.span,
    });
    let Some(T::Jump { arguments, .. }) = &mut function.blocks[source_block].terminator else {
        unreachable!()
    };
    arguments[index] = alias;
    psrs_backend::mir::verify_module(&stages.mir).unwrap();
}

//! A strict initializer and one successful allocation per instance binding.
use super::*;
impl Structurer<'_> {
    pub(super) fn emit_static_cell(
        &self,
        instruction: &MirInstruction,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        let MirInstruction::StaticCell {
            destination,
            binding,
            type_index,
            initial,
            span,
        } = instruction
        else {
            unreachable!("static cell emitter")
        };
        let global = self
            .static_globals
            .get(binding)
            .ok_or_else(|| wasm_error(*span, "static cell has no planned global"))?;
        body.push(Op::Leaf(Instruction::GlobalGet(global.0)));
        body.push(Op::Leaf(Instruction::RefIsNull));
        body.push(Op::Leaf(Instruction::If(wasm_encoder::BlockType::Empty)));
        self.load(body, *initial, *span)?;
        body.push(Op::Leaf(Instruction::ArrayNewFixed {
            array_type_index: type_index.0,
            array_size: 1,
        }));
        body.push(Op::Leaf(Instruction::GlobalSet(global.0)));
        body.push(Op::Leaf(Instruction::End));
        body.push(Op::Leaf(Instruction::GlobalGet(global.0)));
        body.push(Op::Leaf(Instruction::RefAsNonNull));
        self.store(body, *destination, *span)
    }
}

//! The synthesized linear-memory allocator used by canonical ABI lowering.

use super::super::{Body, Function, Op, TypeIndex};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{Instruction, MemArg, ValType};

/// Builds the bump-allocator `cabi_realloc` used to allocate returned
/// `list`/`string` buffers and linear closure environments.
#[allow(clippy::vec_init_then_push)]
pub(super) fn build_realloc(type_index: TypeIndex, heap_pointer: u32, span: TextRange) -> Function {
    let memarg = || MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    };
    let page_round = |body: &mut Body| {
        body.push(Op::Leaf(Instruction::I32Const(65535)));
        body.push(Op::Leaf(Instruction::I32Add));
        body.push(Op::Leaf(Instruction::I32Const(16)));
        body.push(Op::Leaf(Instruction::I32ShrU));
    };
    let mut body = Body::new();
    body.push(Op::Leaf(Instruction::I32Const(heap_pointer as i32)));
    body.push(Op::Leaf(Instruction::I32Load(memarg())));
    body.push(Op::Leaf(Instruction::LocalSet(4)));
    body.push(Op::Leaf(Instruction::LocalGet(4)));
    body.push(Op::Leaf(Instruction::I32Const(4)));
    body.push(Op::Leaf(Instruction::I32Add));
    body.push(Op::Leaf(Instruction::LocalGet(2)));
    body.push(Op::Leaf(Instruction::I32Add));
    body.push(Op::Leaf(Instruction::I32Const(1)));
    body.push(Op::Leaf(Instruction::I32Sub));
    body.push(Op::Leaf(Instruction::I32Const(0)));
    body.push(Op::Leaf(Instruction::LocalGet(2)));
    body.push(Op::Leaf(Instruction::I32Sub));
    body.push(Op::Leaf(Instruction::I32And));
    body.push(Op::Leaf(Instruction::I32Const(4)));
    body.push(Op::Leaf(Instruction::I32Sub));
    body.push(Op::Leaf(Instruction::LocalSet(4)));
    body.push(Op::Leaf(Instruction::LocalGet(4)));
    body.push(Op::Leaf(Instruction::LocalGet(3)));
    body.push(Op::Leaf(Instruction::I32Add));
    body.push(Op::Leaf(Instruction::I32Const(4)));
    body.push(Op::Leaf(Instruction::I32Add));
    body.push(Op::Leaf(Instruction::LocalSet(5)));
    body.push(Op::Leaf(Instruction::LocalGet(5)));
    page_round(&mut body);
    body.push(Op::Leaf(Instruction::MemorySize(0)));
    body.push(Op::Leaf(Instruction::I32GtU));
    let mut grow = Body::new();
    grow.push(Op::Leaf(Instruction::LocalGet(5)));
    page_round(&mut grow);
    grow.push(Op::Leaf(Instruction::MemorySize(0)));
    grow.push(Op::Leaf(Instruction::I32Sub));
    grow.push(Op::Leaf(Instruction::MemoryGrow(0)));
    grow.push(Op::Leaf(Instruction::Drop));
    body.push(Op::If {
        then_body: grow,
        else_body: Body::new(),
        result: None,
        span,
    });
    body.push(Op::Leaf(Instruction::LocalGet(4)));
    body.push(Op::Leaf(Instruction::LocalGet(3)));
    body.push(Op::Leaf(Instruction::I32Store(memarg())));
    body.push(Op::Leaf(Instruction::I32Const(heap_pointer as i32)));
    body.push(Op::Leaf(Instruction::LocalGet(5)));
    body.push(Op::Leaf(Instruction::I32Store(memarg())));
    body.push(Op::Leaf(Instruction::LocalGet(4)));
    body.push(Op::Leaf(Instruction::I32Const(4)));
    body.push(Op::Leaf(Instruction::I32Add));
    Function {
        symbol: SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 1),
        name: "cabi_realloc".into(),
        type_index,
        parameters: vec![ValType::I32; 4],
        locals: vec![ValType::I32, ValType::I32],
        body,
        span,
    }
}

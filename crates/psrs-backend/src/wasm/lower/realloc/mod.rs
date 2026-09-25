//! The synthesized linear-memory allocator used by canonical ABI lowering.

use super::super::{Body, Function, Op, TypeIndex};
use psrs_span::TextRange;
use wasm_encoder::{BlockType, Instruction, MemArg, ValType};

const OLD_PTR: u32 = 0;
const OLD_LEN: u32 = 1;
const ALIGN: u32 = 2;
const NEW_LEN: u32 = 3;
const FREE_PTR: u32 = 4;
const EFFECTIVE_ALIGN: u32 = 5;
const MIN_PAYLOAD: u32 = 6;
const PAYLOAD: u32 = 7;
const PREFIX: u32 = 8;
const END: u32 = 9;
const REQUIRED_PAGES: u32 = 10;
const COPY_REMAINING: u32 = 11;
const COPY_SOURCE: u32 = 12;
const COPY_DESTINATION: u32 = 13;
const BYTE: u32 = 14;

/// Builds the bump-allocator `cabi_realloc` used by canonical ABI boundary
/// buffers, including indirect parameter records and returned lists/strings.
pub(super) fn build_realloc(type_index: TypeIndex, heap_pointer: u32, span: TextRange) -> Function {
    let word = || memarg(2);
    let byte = || memarg(0);
    let mut body = Body::new();

    // Alignment is an unsigned, nonzero power of two. Validate it even for a
    // zero-sized allocation so malformed calls trap consistently.
    let mut invalid_alignment = Body::new();
    push(&mut invalid_alignment, Instruction::LocalGet(ALIGN));
    push(&mut invalid_alignment, Instruction::I32Eqz);
    push(&mut invalid_alignment, Instruction::LocalGet(ALIGN));
    push(&mut invalid_alignment, Instruction::I32Const(1));
    push(&mut invalid_alignment, Instruction::I32Sub);
    push(&mut invalid_alignment, Instruction::LocalGet(ALIGN));
    push(&mut invalid_alignment, Instruction::I32And);
    push(&mut invalid_alignment, Instruction::I32Eqz);
    push(&mut invalid_alignment, Instruction::I32Eqz);
    push(&mut invalid_alignment, Instruction::I32Or);
    trap_if(&mut body, invalid_alignment, span);

    // Free is a no-op. Keep the old allocation untouched and avoid all memory
    // accesses for this case.
    let mut zero_sized = Body::new();
    push(&mut zero_sized, Instruction::I32Const(0));
    push(&mut zero_sized, Instruction::Return);
    body.push(Op::Leaf(Instruction::LocalGet(NEW_LEN)));
    body.push(Op::Leaf(Instruction::I32Eqz));
    body.push(Op::If {
        then_body: zero_sized,
        else_body: Body::new(),
        result: None,
        span,
    });

    // A null old pointer cannot describe bytes to preserve. The old range is
    // represented by a payload pointer preceded by the matching length.
    let mut null_with_length = Body::new();
    push(&mut null_with_length, Instruction::LocalGet(OLD_PTR));
    push(&mut null_with_length, Instruction::I32Eqz);
    push(&mut null_with_length, Instruction::LocalGet(OLD_LEN));
    push(&mut null_with_length, Instruction::I32Eqz);
    push(&mut null_with_length, Instruction::I32Eqz);
    push(&mut null_with_length, Instruction::I32And);
    trap_if(&mut body, null_with_length, span);

    let mut effective_alignment = Body::new();
    push(&mut effective_alignment, Instruction::I32Const(4));
    push(
        &mut effective_alignment,
        Instruction::LocalSet(EFFECTIVE_ALIGN),
    );
    body.push(Op::Leaf(Instruction::LocalGet(ALIGN)));
    body.push(Op::Leaf(Instruction::I32Const(4)));
    body.push(Op::Leaf(Instruction::I32LtU));
    body.push(Op::If {
        then_body: effective_alignment,
        else_body: {
            let mut otherwise = Body::new();
            push(&mut otherwise, Instruction::LocalGet(ALIGN));
            push(&mut otherwise, Instruction::LocalSet(EFFECTIVE_ALIGN));
            otherwise
        },
        result: None,
        span,
    });

    // Confirm that a non-null old allocation has a complete prefix and that
    // the caller's old_len agrees with it. The pointer subtraction is guarded
    // before the prefix load; out-of-memory ranges trap on the load itself.
    body.push(Op::Leaf(Instruction::LocalGet(OLD_PTR)));
    body.push(Op::Leaf(Instruction::I32Eqz));
    body.push(Op::Leaf(Instruction::I32Eqz));
    body.push(Op::If {
        then_body: {
            let mut validate_old = Body::new();
            let mut short_old_pointer = Body::new();
            push(&mut short_old_pointer, Instruction::LocalGet(OLD_PTR));
            push(&mut short_old_pointer, Instruction::I32Const(4));
            push(&mut short_old_pointer, Instruction::I32LtU);
            trap_if(&mut validate_old, short_old_pointer, span);

            let mut old_range_overflows = Body::new();
            push(&mut old_range_overflows, Instruction::LocalGet(OLD_PTR));
            push(&mut old_range_overflows, Instruction::LocalGet(OLD_LEN));
            push(&mut old_range_overflows, Instruction::I32Add);
            push(&mut old_range_overflows, Instruction::LocalTee(FREE_PTR));
            push(&mut old_range_overflows, Instruction::LocalGet(OLD_PTR));
            push(&mut old_range_overflows, Instruction::I32LtU);
            trap_if(&mut validate_old, old_range_overflows, span);

            let mut old_range_exceeds_memory = Body::new();
            push(
                &mut old_range_exceeds_memory,
                Instruction::LocalGet(FREE_PTR),
            );
            push(&mut old_range_exceeds_memory, Instruction::I32Const(16));
            push(&mut old_range_exceeds_memory, Instruction::I32ShrU);
            push(&mut old_range_exceeds_memory, Instruction::MemorySize(0));
            push(&mut old_range_exceeds_memory, Instruction::I32GtU);
            push(
                &mut old_range_exceeds_memory,
                Instruction::LocalGet(FREE_PTR),
            );
            push(&mut old_range_exceeds_memory, Instruction::I32Const(16));
            push(&mut old_range_exceeds_memory, Instruction::I32ShrU);
            push(&mut old_range_exceeds_memory, Instruction::MemorySize(0));
            push(&mut old_range_exceeds_memory, Instruction::I32Eq);
            push(
                &mut old_range_exceeds_memory,
                Instruction::LocalGet(FREE_PTR),
            );
            push(&mut old_range_exceeds_memory, Instruction::I32Const(65535));
            push(&mut old_range_exceeds_memory, Instruction::I32And);
            push(&mut old_range_exceeds_memory, Instruction::I32Eqz);
            push(&mut old_range_exceeds_memory, Instruction::I32Eqz);
            push(&mut old_range_exceeds_memory, Instruction::I32And);
            push(&mut old_range_exceeds_memory, Instruction::I32Or);
            trap_if(&mut validate_old, old_range_exceeds_memory, span);

            let mut mismatched_old_length = Body::new();
            push(&mut mismatched_old_length, Instruction::LocalGet(OLD_PTR));
            push(&mut mismatched_old_length, Instruction::I32Const(4));
            push(&mut mismatched_old_length, Instruction::I32Sub);
            push(&mut mismatched_old_length, Instruction::I32Load(word()));
            push(&mut mismatched_old_length, Instruction::LocalGet(OLD_LEN));
            push(&mut mismatched_old_length, Instruction::I32Ne);
            trap_if(&mut validate_old, mismatched_old_length, span);
            validate_old
        },
        else_body: Body::new(),
        result: None,
        span,
    });

    // Compute an aligned payload pointer without allowing any intermediate
    // wasm32 address calculation to wrap. The four-byte prefix is before the
    // payload, and the end pointer is the next free byte.
    push(&mut body, Instruction::I32Const(heap_pointer as i32));
    push(&mut body, Instruction::I32Load(word()));
    push(&mut body, Instruction::LocalTee(FREE_PTR));
    push(&mut body, Instruction::I32Const(4));
    push(&mut body, Instruction::I32Add);
    push(&mut body, Instruction::LocalTee(MIN_PAYLOAD));
    push(&mut body, Instruction::LocalGet(FREE_PTR));
    push(&mut body, Instruction::I32LtU);
    trap_if_top(&mut body, span);

    push(&mut body, Instruction::LocalGet(MIN_PAYLOAD));
    push(&mut body, Instruction::LocalGet(EFFECTIVE_ALIGN));
    push(&mut body, Instruction::I32Const(1));
    push(&mut body, Instruction::I32Sub);
    push(&mut body, Instruction::I32Add);
    push(&mut body, Instruction::LocalTee(PAYLOAD));
    push(&mut body, Instruction::LocalGet(MIN_PAYLOAD));
    push(&mut body, Instruction::I32LtU);
    trap_if_top(&mut body, span);

    push(&mut body, Instruction::LocalGet(PAYLOAD));
    push(&mut body, Instruction::I32Const(0));
    push(&mut body, Instruction::LocalGet(EFFECTIVE_ALIGN));
    push(&mut body, Instruction::I32Sub);
    push(&mut body, Instruction::I32And);
    push(&mut body, Instruction::LocalSet(PAYLOAD));

    push(&mut body, Instruction::LocalGet(PAYLOAD));
    push(&mut body, Instruction::I32Const(4));
    push(&mut body, Instruction::I32Sub);
    push(&mut body, Instruction::LocalSet(PREFIX));
    push(&mut body, Instruction::LocalGet(PAYLOAD));
    push(&mut body, Instruction::LocalGet(NEW_LEN));
    push(&mut body, Instruction::I32Add);
    push(&mut body, Instruction::LocalTee(END));
    push(&mut body, Instruction::LocalGet(PAYLOAD));
    push(&mut body, Instruction::I32LtU);
    trap_if_top(&mut body, span);

    // ceil(end / 65536), written without adding 65535 (which itself could
    // overflow). A failed memory.grow returns -1 and becomes an unconditional
    // trap before any allocator state is committed.
    push(&mut body, Instruction::LocalGet(END));
    push(&mut body, Instruction::I32Const(16));
    push(&mut body, Instruction::I32ShrU);
    push(&mut body, Instruction::LocalGet(END));
    push(&mut body, Instruction::I32Const(65535));
    push(&mut body, Instruction::I32And);
    push(&mut body, Instruction::I32Eqz);
    push(&mut body, Instruction::I32Eqz);
    push(&mut body, Instruction::I32Add);
    push(&mut body, Instruction::LocalSet(REQUIRED_PAGES));

    push(&mut body, Instruction::LocalGet(REQUIRED_PAGES));
    push(&mut body, Instruction::MemorySize(0));
    push(&mut body, Instruction::I32GtU);
    body.push(Op::If {
        then_body: {
            let mut grow = Body::new();
            push(&mut grow, Instruction::LocalGet(REQUIRED_PAGES));
            push(&mut grow, Instruction::MemorySize(0));
            push(&mut grow, Instruction::I32Sub);
            push(&mut grow, Instruction::MemoryGrow(0));
            push(&mut grow, Instruction::I32Const(-1));
            push(&mut grow, Instruction::I32Eq);
            trap_if_top(&mut grow, span);
            grow
        },
        else_body: Body::new(),
        result: None,
        span,
    });

    // Copy min(old_len, new_len) bytes using MVP byte loads and stores. This
    // preserves realloc contents without relying on bulk-memory opcodes.
    push(&mut body, Instruction::LocalGet(OLD_LEN));
    push(&mut body, Instruction::LocalGet(NEW_LEN));
    push(&mut body, Instruction::I32LtU);
    body.push(Op::If {
        then_body: {
            let mut use_old_length = Body::new();
            push(&mut use_old_length, Instruction::LocalGet(OLD_LEN));
            push(&mut use_old_length, Instruction::LocalSet(COPY_REMAINING));
            use_old_length
        },
        else_body: {
            let mut use_new_length = Body::new();
            push(&mut use_new_length, Instruction::LocalGet(NEW_LEN));
            push(&mut use_new_length, Instruction::LocalSet(COPY_REMAINING));
            use_new_length
        },
        result: None,
        span,
    });
    body.push(Op::Leaf(Instruction::LocalGet(OLD_PTR)));
    body.push(Op::Leaf(Instruction::LocalSet(COPY_SOURCE)));
    body.push(Op::Leaf(Instruction::LocalGet(PAYLOAD)));
    body.push(Op::Leaf(Instruction::LocalSet(COPY_DESTINATION)));
    body.push(Op::Leaf(Instruction::Block(BlockType::Empty)));
    body.push(Op::Leaf(Instruction::Loop(BlockType::Empty)));
    body.push(Op::Leaf(Instruction::LocalGet(COPY_REMAINING)));
    body.push(Op::Leaf(Instruction::I32Eqz));
    body.push(Op::Leaf(Instruction::BrIf(1)));
    body.push(Op::Leaf(Instruction::LocalGet(COPY_SOURCE)));
    body.push(Op::Leaf(Instruction::I32Load8U(byte())));
    body.push(Op::Leaf(Instruction::LocalSet(BYTE)));
    body.push(Op::Leaf(Instruction::LocalGet(COPY_DESTINATION)));
    body.push(Op::Leaf(Instruction::LocalGet(BYTE)));
    body.push(Op::Leaf(Instruction::I32Store8(byte())));
    body.push(Op::Leaf(Instruction::LocalGet(COPY_SOURCE)));
    body.push(Op::Leaf(Instruction::I32Const(1)));
    body.push(Op::Leaf(Instruction::I32Add));
    body.push(Op::Leaf(Instruction::LocalSet(COPY_SOURCE)));
    body.push(Op::Leaf(Instruction::LocalGet(COPY_DESTINATION)));
    body.push(Op::Leaf(Instruction::I32Const(1)));
    body.push(Op::Leaf(Instruction::I32Add));
    body.push(Op::Leaf(Instruction::LocalSet(COPY_DESTINATION)));
    body.push(Op::Leaf(Instruction::LocalGet(COPY_REMAINING)));
    body.push(Op::Leaf(Instruction::I32Const(1)));
    body.push(Op::Leaf(Instruction::I32Sub));
    body.push(Op::Leaf(Instruction::LocalSet(COPY_REMAINING)));
    body.push(Op::Leaf(Instruction::Br(0)));
    body.push(Op::Leaf(Instruction::End));
    body.push(Op::Leaf(Instruction::End));

    push(&mut body, Instruction::LocalGet(PREFIX));
    push(&mut body, Instruction::LocalGet(NEW_LEN));
    push(&mut body, Instruction::I32Store(word()));
    push(&mut body, Instruction::I32Const(heap_pointer as i32));
    push(&mut body, Instruction::LocalGet(END));
    push(&mut body, Instruction::I32Store(word()));
    push(&mut body, Instruction::LocalGet(PAYLOAD));

    Function {
        symbol: crate::abi::REALLOC_SYMBOL,
        name: "cabi_realloc".into(),
        type_index,
        parameters: vec![ValType::I32; 4],
        locals: vec![ValType::I32; 11],
        body,
        span,
    }
}

fn memarg(align: u32) -> MemArg {
    MemArg {
        offset: 0,
        align,
        memory_index: 0,
    }
}

fn push(body: &mut Body, instruction: Instruction<'static>) {
    body.push(Op::Leaf(instruction));
}

fn trap_if(body: &mut Body, condition: Body, span: TextRange) {
    body.extend(condition);
    body.push(Op::If {
        then_body: vec![Op::Leaf(Instruction::Unreachable)],
        else_body: Body::new(),
        result: None,
        span,
    });
}

fn trap_if_top(body: &mut Body, span: TextRange) {
    body.push(Op::If {
        then_body: vec![Op::Leaf(Instruction::Unreachable)],
        else_body: Body::new(),
        result: None,
        span,
    });
}

#[cfg(test)]
mod tests;

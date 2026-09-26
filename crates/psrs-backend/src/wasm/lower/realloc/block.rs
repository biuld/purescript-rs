//! Block allocation, free-list reuse, and coalescing for `cabi_realloc`.

use super::*;
use crate::abi;
use wasm_encoder::Instruction;

/// Allocates `NEW_LEN` bytes aligned to `ALIGN` (rounded up to the block
/// granularity) and leaves the payload pointer in `L_P`.
pub(super) fn emit_allocate(asm: &mut Asm) {
    // a = max(align, MIN_BLOCK)
    let small = asm.label();
    get(asm, ALIGN);
    constant(asm, abi::MIN_BLOCK as i32);
    asm.leaf(Instruction::I32LtU);
    asm.if_(small);
    constant(asm, abi::MIN_BLOCK as i32);
    set(asm, L_A);
    asm.else_();
    get(asm, ALIGN);
    set(asm, L_A);
    asm.end();

    constant(asm, 0);
    set(asm, L_PREV);
    state_load(asm, 0);
    set(asm, L_NODE);

    let found = asm.label();
    let scan = asm.label();
    asm.block(found);
    asm.loop_(scan);
    get(asm, L_NODE);
    asm.leaf(Instruction::I32Eqz);
    asm.br_if(found);
    word_load(asm, L_NODE, 0);
    set(asm, L_SIZE);
    // P = align_up(NODE + HEADER, a)
    get(asm, L_NODE);
    constant(asm, abi::HEADER_SIZE as i32);
    asm.leaf(Instruction::I32Add);
    set(asm, L_PNODE);
    local_trap_if_wrapped(asm, L_PNODE, L_NODE);
    checked_align_up(asm, L_PNODE, L_A);
    // front = align_up((P - NODE) + NEW_LEN, MIN_BLOCK)
    get(asm, L_PNODE);
    get(asm, L_NODE);
    asm.leaf(Instruction::I32Sub);
    get(asm, NEW_LEN);
    asm.leaf(Instruction::I32Add);
    set(asm, L_FRONT);
    checked_align_up_const(asm, L_FRONT, abi::MIN_BLOCK);
    // front <= size: this block fits
    get(asm, L_FRONT);
    get(asm, L_SIZE);
    asm.leaf(Instruction::I32LeU);
    asm.br_if(found);
    get(asm, L_NODE);
    set(asm, L_PREV);
    word_load(asm, L_NODE, 4);
    set(asm, L_NODE);
    asm.br(scan);
    asm.end();
    asm.end();

    let bump = asm.label();
    get(asm, L_NODE);
    asm.leaf(Instruction::I32Eqz);
    asm.if_(bump);
    emit_bump(asm);
    asm.else_();
    emit_reuse(asm);
    asm.end();
}

/// Carves a fresh block off the bump end, growing memory when needed, and
/// leaves the payload pointer in `L_P`.
fn emit_bump(asm: &mut Asm) {
    state_load(asm, 4);
    set(asm, L_BREAK);
    // P = align_up(BREAK + HEADER, a)
    get(asm, L_BREAK);
    constant(asm, abi::HEADER_SIZE as i32);
    asm.leaf(Instruction::I32Add);
    set(asm, L_P);
    local_trap_if_wrapped(asm, L_P, L_BREAK);
    checked_align_up(asm, L_P, L_A);
    // END = align_up(P + NEW_LEN, MIN_BLOCK)
    get(asm, L_P);
    get(asm, NEW_LEN);
    asm.leaf(Instruction::I32Add);
    set(asm, L_END);
    local_trap_if_wrapped(asm, L_END, L_P);
    checked_align_up_const(asm, L_END, abi::MIN_BLOCK);
    // block_size = END - BREAK
    get(asm, L_END);
    get(asm, L_BREAK);
    asm.leaf(Instruction::I32Sub);
    set(asm, L_TMP);
    word_store(asm, L_BREAK, 0, L_TMP);
    word_store(asm, L_BREAK, 4, NEW_LEN);
    // pages = ceil(END / 65536)
    get(asm, L_END);
    constant(asm, 16);
    asm.leaf(Instruction::I32ShrU);
    get(asm, L_END);
    constant(asm, 0xffff);
    asm.leaf(Instruction::I32And);
    asm.leaf(Instruction::I32Eqz);
    asm.leaf(Instruction::I32Eqz);
    asm.leaf(Instruction::I32Add);
    set(asm, L_PAGES);
    let grow = asm.label();
    get(asm, L_PAGES);
    asm.leaf(Instruction::MemorySize(0));
    asm.leaf(Instruction::I32GtU);
    asm.if_(grow);
    get(asm, L_PAGES);
    asm.leaf(Instruction::MemorySize(0));
    asm.leaf(Instruction::I32Sub);
    asm.leaf(Instruction::MemoryGrow(0));
    constant(asm, -1);
    asm.leaf(Instruction::I32Eq);
    let failed = asm.label();
    asm.if_(failed);
    asm.leaf(Instruction::Unreachable);
    asm.end();
    asm.end();
    state_store(asm, 4, L_END);
}

/// Takes the first-fitting block `L_NODE`, splitting off the tail when the
/// remainder can hold another block, and leaves the payload pointer in `L_P`.
fn emit_reuse(asm: &mut Asm) {
    let split = asm.label();
    get(asm, L_SIZE);
    get(asm, L_FRONT);
    asm.leaf(Instruction::I32Sub);
    constant(asm, abi::MIN_BLOCK as i32);
    asm.leaf(Instruction::I32GeU);
    asm.if_(split);
    get(asm, L_NODE);
    get(asm, L_FRONT);
    asm.leaf(Instruction::I32Add);
    set(asm, L_TAIL);
    get(asm, L_SIZE);
    get(asm, L_FRONT);
    asm.leaf(Instruction::I32Sub);
    set(asm, L_TMP);
    word_store(asm, L_TAIL, 0, L_TMP);
    word_load(asm, L_NODE, 4);
    set(asm, L_TMP2);
    word_store(asm, L_TAIL, 4, L_TMP2);
    link_replace(asm, L_TAIL);
    word_store(asm, L_NODE, 0, L_FRONT);
    asm.else_();
    word_load(asm, L_NODE, 4);
    set(asm, L_TMP2);
    link_replace(asm, L_TMP2);
    asm.end();
    word_store(asm, L_NODE, 4, NEW_LEN);
    get(asm, L_PNODE);
    set(asm, L_P);
}

/// Makes `PREV.next` (or the free-list head when `PREV == 0`) point at `L_NODE`.
fn link_replace(asm: &mut Asm, new_node: u32) {
    let head = asm.label();
    get(asm, L_PREV);
    asm.leaf(Instruction::I32Eqz);
    asm.if_(head);
    state_store(asm, 0, new_node);
    asm.else_();
    word_store(asm, L_PREV, 4, new_node);
    asm.end();
}

/// Frees the payload pointer in `fp`, coalescing with adjacent free blocks.
pub(super) fn emit_free(asm: &mut Asm, fp: u32) {
    get(asm, fp);
    constant(asm, abi::HEADER_SIZE as i32);
    asm.leaf(Instruction::I32Sub);
    set(asm, L_H);
    word_load(asm, L_H, 0);
    set(asm, L_SIZE);

    constant(asm, 0);
    set(asm, L_PREV);
    state_load(asm, 0);
    set(asm, L_NODE);
    let done = asm.label();
    let scan = asm.label();
    asm.block(done);
    asm.loop_(scan);
    get(asm, L_NODE);
    asm.leaf(Instruction::I32Eqz);
    asm.br_if(done);
    get(asm, L_NODE);
    get(asm, L_H);
    asm.leaf(Instruction::I32GeU);
    asm.br_if(done);
    get(asm, L_NODE);
    set(asm, L_PREV);
    word_load(asm, L_NODE, 4);
    set(asm, L_NODE);
    asm.br(scan);
    asm.end();
    asm.end();

    // Merge with the following free block when adjacent.
    let has_next = asm.label();
    get(asm, L_NODE);
    asm.leaf(Instruction::I32Eqz);
    asm.leaf(Instruction::I32Eqz);
    asm.if_(has_next);
    let adjacent_next = asm.label();
    get(asm, L_H);
    get(asm, L_SIZE);
    asm.leaf(Instruction::I32Add);
    get(asm, L_NODE);
    asm.leaf(Instruction::I32Eq);
    asm.if_(adjacent_next);
    word_load(asm, L_NODE, 0);
    set(asm, L_TMP);
    get(asm, L_SIZE);
    get(asm, L_TMP);
    asm.leaf(Instruction::I32Add);
    set(asm, L_SIZE);
    word_load(asm, L_NODE, 4);
    set(asm, L_NODE);
    asm.end();
    asm.end();

    // Merge with the preceding free block when adjacent, otherwise insert.
    let has_prev = asm.label();
    get(asm, L_PREV);
    asm.leaf(Instruction::I32Eqz);
    asm.leaf(Instruction::I32Eqz);
    asm.if_(has_prev);
    let adjacent_prev = asm.label();
    get(asm, L_PREV);
    word_load(asm, L_PREV, 0);
    asm.leaf(Instruction::I32Add);
    get(asm, L_H);
    asm.leaf(Instruction::I32Eq);
    asm.if_(adjacent_prev);
    word_load(asm, L_PREV, 0);
    get(asm, L_SIZE);
    asm.leaf(Instruction::I32Add);
    set(asm, L_TMP);
    word_store(asm, L_PREV, 0, L_TMP);
    word_store(asm, L_PREV, 4, L_NODE);
    asm.else_();
    emit_insert(asm);
    asm.end();
    asm.else_();
    emit_insert(asm);
    asm.end();
}

/// Inserts the free block at `L_H` before `L_NODE` in the address-ordered list.
fn emit_insert(asm: &mut Asm) {
    word_store(asm, L_H, 0, L_SIZE);
    word_store(asm, L_H, 4, L_NODE);
    link_replace(asm, L_H);
}

/// Rounds the local `value` up to the power-of-two `align` local, trapping on
/// pointer-width overflow.
fn checked_align_up(asm: &mut Asm, value: u32, align: u32) {
    get(asm, value);
    get(asm, align);
    constant(asm, 1);
    asm.leaf(Instruction::I32Sub);
    asm.leaf(Instruction::I32Add);
    set(asm, L_TMP);
    local_trap_if_wrapped(asm, L_TMP, value);
    get(asm, L_TMP);
    constant(asm, 0);
    get(asm, align);
    asm.leaf(Instruction::I32Sub);
    asm.leaf(Instruction::I32And);
    set(asm, value);
}

/// Rounds the local `value` up to the power-of-two constant `align`, trapping on
/// pointer-width overflow.
fn checked_align_up_const(asm: &mut Asm, value: u32, align: u32) {
    get(asm, value);
    constant(asm, (align - 1) as i32);
    asm.leaf(Instruction::I32Add);
    set(asm, L_TMP);
    local_trap_if_wrapped(asm, L_TMP, value);
    get(asm, L_TMP);
    constant(asm, (0i32).wrapping_sub(align as i32));
    asm.leaf(Instruction::I32And);
    set(asm, value);
}

/// Traps when `result` is numerically below `source`, i.e. an add wrapped.
fn local_trap_if_wrapped(asm: &mut Asm, result: u32, source: u32) {
    trap_if(asm, |a| {
        get(a, result);
        get(a, source);
        a.leaf(Instruction::I32LtU);
    });
}

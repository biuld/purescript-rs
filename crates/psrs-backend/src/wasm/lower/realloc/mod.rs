//! The synthesized linear-memory allocator used by canonical ABI lowering.
//!
//! `cabi_realloc` implements the full canonical `(old_ptr, old_len, align,
//! new_len) -> pointer` contract: it allocates an aligned block, frees when the
//! new length is zero, and resizes with copy otherwise. Freed blocks go on an
//! address-ordered free list and are coalesced with their neighbors, so a
//! request reuses reclaimed storage instead of exhausting memory. The block
//! layout and ownership rules are fixed by
//! `docs/design/backend/wasm/canonical-buffer-allocation-and-lifetime.md`.

use super::super::{Function, TypeIndex};
use super::asm::*;
use crate::abi;
use psrs_span::TextRange;
use wasm_encoder::{Instruction, MemArg, ValType};

mod block;

use block::{emit_allocate, emit_free};

pub(super) const OLD_PTR: u32 = 0;
pub(super) const OLD_LEN: u32 = 1;
pub(super) const ALIGN: u32 = 2;
pub(super) const NEW_LEN: u32 = 3;

// Scratch locals. Parameters occupy 0..4, so the first scratch local is 4.
pub(super) const L_A: u32 = 4;
pub(super) const L_PREV: u32 = 5;
pub(super) const L_NODE: u32 = 6;
pub(super) const L_SIZE: u32 = 7;
pub(super) const L_PNODE: u32 = 8;
pub(super) const L_FRONT: u32 = 9;
pub(super) const L_TAIL: u32 = 10;
pub(super) const L_P: u32 = 11;
pub(super) const L_END: u32 = 12;
pub(super) const L_PAGES: u32 = 13;
pub(super) const L_BREAK: u32 = 14;
pub(super) const L_SRC: u32 = 15;
pub(super) const L_DST: u32 = 16;
pub(super) const L_BYTE: u32 = 17;
pub(super) const L_COUNT: u32 = 18;
pub(super) const L_H: u32 = 19;
pub(super) const L_CAP: u32 = 20;
pub(super) const L_TMP: u32 = 21;
pub(super) const L_TMP2: u32 = 22;
pub(super) const LOCAL_COUNT: usize = 19;

pub(super) const WORD_ALIGN: u32 = 2;
pub(super) const BYTE_ALIGN: u32 = 0;

/// Builds the reclaiming `cabi_realloc` used by canonical ABI boundary buffers,
/// including indirect parameter records and returned lists/strings.
pub(super) fn build_realloc(type_index: TypeIndex, span: TextRange) -> Function {
    let mut asm = Asm::new();

    // Alignment is an unsigned, nonzero power of two. Validate it even for a
    // zero-sized allocation so malformed calls trap consistently.
    trap_if(&mut asm, |a| {
        get(a, ALIGN);
        a.leaf(Instruction::I32Eqz);
        get(a, ALIGN);
        constant(a, 1);
        a.leaf(Instruction::I32Sub);
        get(a, ALIGN);
        a.leaf(Instruction::I32And);
        a.leaf(Instruction::I32Eqz);
        a.leaf(Instruction::I32Eqz);
        a.leaf(Instruction::I32Or);
    });

    // new_len == 0 frees the old block (when any) and returns null.
    let zero_length = asm.label();
    get(&mut asm, NEW_LEN);
    asm.leaf(Instruction::I32Eqz);
    asm.if_(zero_length);
    {
        let has_old = asm.label();
        get(&mut asm, OLD_PTR);
        asm.leaf(Instruction::I32Eqz);
        asm.leaf(Instruction::I32Eqz);
        asm.if_(has_old);
        emit_free(&mut asm, OLD_PTR);
        asm.end();
        constant(&mut asm, 0);
        asm.leaf(Instruction::Return);
    }
    asm.end();

    // A non-null old pointer must describe a complete block whose stored length
    // agrees with the caller's. If the block already has capacity and alignment
    // for the new length, resize in place.
    let old_nonnull = asm.label();
    get(&mut asm, OLD_PTR);
    asm.leaf(Instruction::I32Eqz);
    asm.leaf(Instruction::I32Eqz);
    asm.if_(old_nonnull);
    {
        trap_if(&mut asm, |a| {
            get(a, OLD_PTR);
            constant(a, abi::HEADER_SIZE as i32);
            a.leaf(Instruction::I32LtU);
        });
        trap_if(&mut asm, |a| {
            get(a, OLD_PTR);
            constant(a, 4);
            a.leaf(Instruction::I32Sub);
            a.leaf(Instruction::I32Load(word_memarg()));
            get(a, OLD_LEN);
            a.leaf(Instruction::I32Ne);
        });
        get(&mut asm, OLD_PTR);
        constant(&mut asm, abi::HEADER_SIZE as i32);
        asm.leaf(Instruction::I32Sub);
        set(&mut asm, L_H);
        word_load(&mut asm, L_H, 0);
        set(&mut asm, L_SIZE);
        get(&mut asm, L_SIZE);
        constant(&mut asm, abi::HEADER_SIZE as i32);
        asm.leaf(Instruction::I32Sub);
        set(&mut asm, L_CAP);

        let fits_in_place = asm.label();
        get(&mut asm, NEW_LEN);
        get(&mut asm, L_CAP);
        asm.leaf(Instruction::I32LeU);
        get(&mut asm, OLD_PTR);
        get(&mut asm, ALIGN);
        constant(&mut asm, 1);
        asm.leaf(Instruction::I32Sub);
        asm.leaf(Instruction::I32And);
        asm.leaf(Instruction::I32Eqz);
        asm.leaf(Instruction::I32And);
        asm.if_(fits_in_place);
        {
            get(&mut asm, OLD_PTR);
            constant(&mut asm, 4);
            asm.leaf(Instruction::I32Sub);
            get(&mut asm, NEW_LEN);
            asm.leaf(Instruction::I32Store(word_memarg()));
            get(&mut asm, OLD_PTR);
            asm.leaf(Instruction::Return);
        }
        asm.end();
    }
    asm.end();

    // A null old pointer requires an empty old length.
    trap_if(&mut asm, |a| {
        get(a, OLD_PTR);
        a.leaf(Instruction::I32Eqz);
        get(a, OLD_LEN);
        a.leaf(Instruction::I32Eqz);
        a.leaf(Instruction::I32Eqz);
        a.leaf(Instruction::I32And);
    });

    emit_allocate(&mut asm);

    let has_old_bytes = asm.label();
    get(&mut asm, OLD_PTR);
    asm.leaf(Instruction::I32Eqz);
    asm.leaf(Instruction::I32Eqz);
    asm.if_(has_old_bytes);
    {
        let old_is_smaller = asm.label();
        get(&mut asm, OLD_LEN);
        get(&mut asm, NEW_LEN);
        asm.leaf(Instruction::I32LtU);
        asm.if_(old_is_smaller);
        get(&mut asm, OLD_LEN);
        set(&mut asm, L_COUNT);
        asm.else_();
        get(&mut asm, NEW_LEN);
        set(&mut asm, L_COUNT);
        asm.end();
        get(&mut asm, OLD_PTR);
        set(&mut asm, L_SRC);
        get(&mut asm, L_P);
        set(&mut asm, L_DST);
        let done = asm.label();
        let next = asm.label();
        asm.block(done);
        asm.loop_(next);
        get(&mut asm, L_COUNT);
        asm.leaf(Instruction::I32Eqz);
        asm.br_if(done);
        get(&mut asm, L_SRC);
        asm.leaf(Instruction::I32Load8U(byte_memarg()));
        set(&mut asm, L_BYTE);
        get(&mut asm, L_DST);
        get(&mut asm, L_BYTE);
        asm.leaf(Instruction::I32Store8(byte_memarg()));
        advance(&mut asm, L_SRC, 1);
        advance(&mut asm, L_DST, 1);
        get(&mut asm, L_COUNT);
        constant(&mut asm, 1);
        asm.leaf(Instruction::I32Sub);
        set(&mut asm, L_COUNT);
        asm.br(next);
        asm.end();
        asm.end();
        emit_free(&mut asm, OLD_PTR);
    }
    asm.end();

    get(&mut asm, L_P);

    Function {
        symbol: crate::abi::REALLOC_SYMBOL,
        name: "cabi_realloc".into(),
        type_index,
        parameters: vec![ValType::I32; 4],
        locals: vec![ValType::I32; LOCAL_COUNT],
        body: asm.into_body(),
        span,
    }
}

pub(super) fn word_memarg() -> MemArg {
    MemArg {
        offset: 0,
        align: WORD_ALIGN,
        memory_index: 0,
    }
}

pub(super) fn byte_memarg() -> MemArg {
    MemArg {
        offset: 0,
        align: BYTE_ALIGN,
        memory_index: 0,
    }
}

pub(super) fn word_load(asm: &mut Asm, base: u32, offset: u32) {
    get(asm, base);
    if offset != 0 {
        constant(asm, offset as i32);
        asm.leaf(Instruction::I32Add);
    }
    asm.leaf(Instruction::I32Load(word_memarg()));
}

pub(super) fn word_store(asm: &mut Asm, base: u32, offset: u32, value: u32) {
    get(asm, base);
    if offset != 0 {
        constant(asm, offset as i32);
        asm.leaf(Instruction::I32Add);
    }
    get(asm, value);
    asm.leaf(Instruction::I32Store(word_memarg()));
}

pub(super) fn state_load(asm: &mut Asm, offset: u32) {
    constant(asm, (abi::HEAP_STATE + offset) as i32);
    asm.leaf(Instruction::I32Load(word_memarg()));
}

pub(super) fn state_store(asm: &mut Asm, offset: u32, value: u32) {
    constant(asm, (abi::HEAP_STATE + offset) as i32);
    get(asm, value);
    asm.leaf(Instruction::I32Store(word_memarg()));
}

pub(super) fn trap_if(asm: &mut Asm, condition: impl FnOnce(&mut Asm)) {
    condition(asm);
    let failed = asm.label();
    asm.if_(failed);
    asm.leaf(Instruction::Unreachable);
    asm.end();
}

#[cfg(test)]
mod tests;

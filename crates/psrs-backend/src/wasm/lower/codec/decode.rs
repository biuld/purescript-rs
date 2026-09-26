//! UTF-8 to UTF-16 decoding into a fresh GC string at the canonical ABI boundary.

use crate::abi::DECODE_STEP_SYMBOL;
use crate::types::DefinedTypeId;
use crate::wasm::lower::asm::*;
use crate::wasm::{Function, FunctionIndex, TypeIndex};
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};

const REPLACEMENT: i32 = 0xFFFD;

/// Packs the result of one decode step.
fn pack(consumed: i32, units: i32, code_point: i32) -> i32 {
    (code_point << 6) | (units << 3) | consumed
}

pub(super) fn bytes_to_string(
    string_type: DefinedTypeId,
    decode_step_index: FunctionIndex,
    type_index: TypeIndex,
    span: TextRange,
) -> Function {
    const PTR: u32 = 0;
    const LEN: u32 = 1;
    const SRC: u32 = 2;
    const REMAINING: u32 = 3;
    const TMP: u32 = 4;
    const DST: u32 = 5;
    const R: u32 = 6;
    const CONSUMED: u32 = 7;
    const UNITS: u32 = 8;
    const CP: u32 = 9;
    const CP2: u32 = 10;
    const EXACT: u32 = 11;

    let mut asm = Asm::new();

    get(&mut asm, PTR);
    set(&mut asm, SRC);
    get(&mut asm, LEN);
    set(&mut asm, REMAINING);

    // tmp = array.new_default len (an upper bound on the code-unit count)
    get(&mut asm, LEN);
    asm.leaf(Instruction::ArrayNewDefault(string_type.0));
    set(&mut asm, TMP);

    constant(&mut asm, 0);
    set(&mut asm, DST);

    let done = asm.label();
    let next = asm.label();
    asm.block(done);
    asm.loop_(next);

    get(&mut asm, REMAINING);
    asm.leaf(Instruction::I32Eqz);
    asm.br_if(done);

    get(&mut asm, SRC);
    get(&mut asm, REMAINING);
    asm.leaf(Instruction::Call(decode_step_index.0));
    set(&mut asm, R);

    get(&mut asm, R);
    constant(&mut asm, 7);
    asm.leaf(Instruction::I32And);
    set(&mut asm, CONSUMED);

    get(&mut asm, R);
    constant(&mut asm, 3);
    asm.leaf(Instruction::I32ShrU);
    constant(&mut asm, 7);
    asm.leaf(Instruction::I32And);
    set(&mut asm, UNITS);

    get(&mut asm, R);
    constant(&mut asm, 6);
    asm.leaf(Instruction::I32ShrU);
    set(&mut asm, CP);

    get(&mut asm, UNITS);
    constant(&mut asm, 1);
    asm.leaf(Instruction::I32Eq);
    let single = asm.label();
    asm.if_(single);
    get(&mut asm, TMP);
    get(&mut asm, DST);
    get(&mut asm, CP);
    asm.leaf(Instruction::ArraySet(string_type.0));
    advance(&mut asm, DST, 1);
    asm.else_();
    // A four-byte sequence becomes a UTF-16 surrogate pair.
    get(&mut asm, CP);
    constant(&mut asm, 0x10000);
    asm.leaf(Instruction::I32Sub);
    set(&mut asm, CP2);
    get(&mut asm, TMP);
    get(&mut asm, DST);
    constant(&mut asm, 0xD800);
    get(&mut asm, CP2);
    constant(&mut asm, 10);
    asm.leaf(Instruction::I32ShrU);
    asm.leaf(Instruction::I32Or);
    asm.leaf(Instruction::ArraySet(string_type.0));
    get(&mut asm, TMP);
    get(&mut asm, DST);
    constant(&mut asm, 1);
    asm.leaf(Instruction::I32Add);
    constant(&mut asm, 0xDC00);
    get(&mut asm, CP2);
    constant(&mut asm, 0x3FF);
    asm.leaf(Instruction::I32And);
    asm.leaf(Instruction::I32Or);
    asm.leaf(Instruction::ArraySet(string_type.0));
    advance(&mut asm, DST, 2);
    asm.end();

    get(&mut asm, SRC);
    get(&mut asm, CONSUMED);
    asm.leaf(Instruction::I32Add);
    set(&mut asm, SRC);
    get(&mut asm, REMAINING);
    get(&mut asm, CONSUMED);
    asm.leaf(Instruction::I32Sub);
    set(&mut asm, REMAINING);
    asm.br(next);

    asm.end();
    asm.end();

    // Return the buffer unchanged when it was already exact.
    get(&mut asm, DST);
    get(&mut asm, LEN);
    asm.leaf(Instruction::I32Eq);
    let already_exact = asm.label();
    asm.if_(already_exact);
    get(&mut asm, TMP);
    asm.leaf(Instruction::RefAsNonNull);
    asm.leaf(Instruction::Return);
    asm.end();

    get(&mut asm, DST);
    asm.leaf(Instruction::ArrayNewDefault(string_type.0));
    set(&mut asm, EXACT);

    get(&mut asm, EXACT);
    constant(&mut asm, 0);
    get(&mut asm, TMP);
    constant(&mut asm, 0);
    get(&mut asm, DST);
    asm.leaf(Instruction::ArrayCopy {
        array_type_index_dst: string_type.0,
        array_type_index_src: string_type.0,
    });

    get(&mut asm, EXACT);
    asm.leaf(Instruction::RefAsNonNull);

    Function {
        symbol: crate::abi::BYTES_TO_STRING_SYMBOL,
        name: "bytes_to_string".into(),
        type_index,
        parameters: vec![ValType::I32, ValType::I32],
        locals: vec![
            ValType::I32,                     // src
            ValType::I32,                     // remaining
            nullable_string_ref(string_type), // tmp
            ValType::I32,                     // dst
            ValType::I32,                     // r
            ValType::I32,                     // consumed
            ValType::I32,                     // units
            ValType::I32,                     // cp
            ValType::I32,                     // cp2
            nullable_string_ref(string_type), // exact
        ],
        body: asm.into_body(),
        span,
    }
}

pub(super) fn decode_step(type_index: TypeIndex, span: TextRange) -> Function {
    const PTR: u32 = 0;
    const REMAINING: u32 = 1;
    const B0: u32 = 2;
    const B1: u32 = 3;
    const B2: u32 = 4;
    const B3: u32 = 5;
    const CP: u32 = 6;

    let mut asm = Asm::new();

    // b0 = load8_u(ptr)
    get(&mut asm, PTR);
    asm.leaf(Instruction::I32Load8U(memarg(0)));
    set(&mut asm, B0);

    // ASCII
    get(&mut asm, B0);
    constant(&mut asm, 0x80);
    asm.leaf(Instruction::I32LtU);
    let ascii = asm.label();
    asm.if_(ascii);
    get(&mut asm, B0);
    constant(&mut asm, 6);
    asm.leaf(Instruction::I32Shl);
    constant(&mut asm, pack(1, 1, 0));
    asm.leaf(Instruction::I32Or);
    asm.leaf(Instruction::Return);
    asm.end();

    // 0x80..=0xC1 is always invalid.
    get(&mut asm, B0);
    constant(&mut asm, 0xC2);
    asm.leaf(Instruction::I32LtU);
    let invalid_lead = asm.label();
    asm.if_(invalid_lead);
    constant(&mut asm, pack(1, 1, REPLACEMENT));
    asm.leaf(Instruction::Return);
    asm.end();

    // Two-byte: 0xC2..=0xDF
    get(&mut asm, B0);
    constant(&mut asm, 0xE0);
    asm.leaf(Instruction::I32LtU);
    let two = asm.label();
    asm.if_(two);
    // remaining < 2 -> invalid
    get(&mut asm, REMAINING);
    constant(&mut asm, 2);
    asm.leaf(Instruction::I32LtU);
    let two_short = asm.label();
    asm.if_(two_short);
    constant(&mut asm, pack(1, 1, REPLACEMENT));
    asm.leaf(Instruction::Return);
    asm.end();
    get(&mut asm, PTR);
    asm.leaf(Instruction::I32Load8U(memarg(1)));
    set(&mut asm, B1);
    // (b1 & 0xC0) != 0x80 -> invalid
    get(&mut asm, B1);
    constant(&mut asm, 0xC0);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 0x80);
    asm.leaf(Instruction::I32Ne);
    let two_bad = asm.label();
    asm.if_(two_bad);
    constant(&mut asm, pack(1, 1, REPLACEMENT));
    asm.leaf(Instruction::Return);
    asm.end();
    // cp = ((b0 & 0x1F) << 6) | (b1 & 0x3F)
    get(&mut asm, B0);
    constant(&mut asm, 0x1F);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 6);
    asm.leaf(Instruction::I32Shl);
    get(&mut asm, B1);
    constant(&mut asm, 0x3F);
    asm.leaf(Instruction::I32And);
    asm.leaf(Instruction::I32Or);
    set(&mut asm, CP);
    get(&mut asm, CP);
    constant(&mut asm, 6);
    asm.leaf(Instruction::I32Shl);
    constant(&mut asm, pack(2, 1, 0));
    asm.leaf(Instruction::I32Or);
    asm.leaf(Instruction::Return);
    asm.end();

    // Three-byte: 0xE0..=0xEF
    get(&mut asm, B0);
    constant(&mut asm, 0xF0);
    asm.leaf(Instruction::I32LtU);
    let three = asm.label();
    asm.if_(three);
    get(&mut asm, REMAINING);
    constant(&mut asm, 3);
    asm.leaf(Instruction::I32LtU);
    let three_short = asm.label();
    asm.if_(three_short);
    constant(&mut asm, pack(1, 1, REPLACEMENT));
    asm.leaf(Instruction::Return);
    asm.end();
    get(&mut asm, PTR);
    asm.leaf(Instruction::I32Load8U(memarg(1)));
    set(&mut asm, B1);
    get(&mut asm, PTR);
    asm.leaf(Instruction::I32Load8U(memarg(2)));
    set(&mut asm, B2);
    // any non-continuation byte -> invalid
    get(&mut asm, B1);
    constant(&mut asm, 0xC0);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 0x80);
    asm.leaf(Instruction::I32Ne);
    get(&mut asm, B2);
    constant(&mut asm, 0xC0);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 0x80);
    asm.leaf(Instruction::I32Ne);
    asm.leaf(Instruction::I32Or);
    let three_cont = asm.label();
    asm.if_(three_cont);
    constant(&mut asm, pack(1, 1, REPLACEMENT));
    asm.leaf(Instruction::Return);
    asm.end();
    get(&mut asm, B0);
    constant(&mut asm, 0x0F);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 12);
    asm.leaf(Instruction::I32Shl);
    get(&mut asm, B1);
    constant(&mut asm, 0x3F);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 6);
    asm.leaf(Instruction::I32Shl);
    asm.leaf(Instruction::I32Or);
    get(&mut asm, B2);
    constant(&mut asm, 0x3F);
    asm.leaf(Instruction::I32And);
    asm.leaf(Instruction::I32Or);
    set(&mut asm, CP);
    // overlong or surrogate -> invalid
    get(&mut asm, CP);
    constant(&mut asm, 0x800);
    asm.leaf(Instruction::I32LtU);
    get(&mut asm, CP);
    constant(&mut asm, 0xD800);
    asm.leaf(Instruction::I32GeU);
    get(&mut asm, CP);
    constant(&mut asm, 0xE000);
    asm.leaf(Instruction::I32LtU);
    asm.leaf(Instruction::I32And);
    asm.leaf(Instruction::I32Or);
    let three_range = asm.label();
    asm.if_(three_range);
    constant(&mut asm, pack(1, 1, REPLACEMENT));
    asm.leaf(Instruction::Return);
    asm.end();
    get(&mut asm, CP);
    constant(&mut asm, 6);
    asm.leaf(Instruction::I32Shl);
    constant(&mut asm, pack(3, 1, 0));
    asm.leaf(Instruction::I32Or);
    asm.leaf(Instruction::Return);
    asm.end();

    // Four-byte: 0xF0..=0xF4
    get(&mut asm, B0);
    constant(&mut asm, 0xF5);
    asm.leaf(Instruction::I32LtU);
    let four = asm.label();
    asm.if_(four);
    get(&mut asm, REMAINING);
    constant(&mut asm, 4);
    asm.leaf(Instruction::I32LtU);
    let four_short = asm.label();
    asm.if_(four_short);
    constant(&mut asm, pack(1, 1, REPLACEMENT));
    asm.leaf(Instruction::Return);
    asm.end();
    get(&mut asm, PTR);
    asm.leaf(Instruction::I32Load8U(memarg(1)));
    set(&mut asm, B1);
    get(&mut asm, PTR);
    asm.leaf(Instruction::I32Load8U(memarg(2)));
    set(&mut asm, B2);
    get(&mut asm, PTR);
    asm.leaf(Instruction::I32Load8U(memarg(3)));
    set(&mut asm, B3);
    get(&mut asm, B1);
    constant(&mut asm, 0xC0);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 0x80);
    asm.leaf(Instruction::I32Ne);
    get(&mut asm, B2);
    constant(&mut asm, 0xC0);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 0x80);
    asm.leaf(Instruction::I32Ne);
    asm.leaf(Instruction::I32Or);
    get(&mut asm, B3);
    constant(&mut asm, 0xC0);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 0x80);
    asm.leaf(Instruction::I32Ne);
    asm.leaf(Instruction::I32Or);
    let four_cont = asm.label();
    asm.if_(four_cont);
    constant(&mut asm, pack(1, 1, REPLACEMENT));
    asm.leaf(Instruction::Return);
    asm.end();
    get(&mut asm, B0);
    constant(&mut asm, 0x07);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 18);
    asm.leaf(Instruction::I32Shl);
    get(&mut asm, B1);
    constant(&mut asm, 0x3F);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 12);
    asm.leaf(Instruction::I32Shl);
    asm.leaf(Instruction::I32Or);
    get(&mut asm, B2);
    constant(&mut asm, 0x3F);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 6);
    asm.leaf(Instruction::I32Shl);
    asm.leaf(Instruction::I32Or);
    get(&mut asm, B3);
    constant(&mut asm, 0x3F);
    asm.leaf(Instruction::I32And);
    asm.leaf(Instruction::I32Or);
    set(&mut asm, CP);
    get(&mut asm, CP);
    constant(&mut asm, 0x10000);
    asm.leaf(Instruction::I32LtU);
    get(&mut asm, CP);
    constant(&mut asm, 0x10FFFF);
    asm.leaf(Instruction::I32GtU);
    asm.leaf(Instruction::I32Or);
    let four_range = asm.label();
    asm.if_(four_range);
    constant(&mut asm, pack(1, 1, REPLACEMENT));
    asm.leaf(Instruction::Return);
    asm.end();
    get(&mut asm, CP);
    constant(&mut asm, 6);
    asm.leaf(Instruction::I32Shl);
    constant(&mut asm, pack(4, 2, 0));
    asm.leaf(Instruction::I32Or);
    asm.leaf(Instruction::Return);
    asm.end();

    // Every remaining lead byte is invalid.
    constant(&mut asm, pack(1, 1, REPLACEMENT));

    Function {
        symbol: DECODE_STEP_SYMBOL,
        name: "decode_step".into(),
        type_index,
        parameters: vec![ValType::I32, ValType::I32],
        locals: vec![ValType::I32; 5],
        body: asm.into_body(),
        span,
    }
}

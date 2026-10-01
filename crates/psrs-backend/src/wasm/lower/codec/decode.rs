//! Strict UTF-8 validation and a byte-exact copy into a GC string.

use crate::abi::VALIDATE_STEP_SYMBOL;
use crate::types::DefinedTypeId;
use crate::wasm::lower::asm::*;
use crate::wasm::{Function, FunctionIndex, TypeIndex};
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};

/// Copies validated linear UTF-8 bytes into a fresh exact-length GC string.
///
/// The bytes are strictly validated first: malformed text rejects the value at
/// the boundary and is never replaced with U+FFFD. Valid bytes are then copied
/// unchanged, so the guest string holds exactly the host's UTF-8.
pub(super) fn bytes_to_string(
    string_type: DefinedTypeId,
    validate_step_index: FunctionIndex,
    type_index: TypeIndex,
    span: TextRange,
) -> Function {
    const PTR: u32 = 0;
    const LEN: u32 = 1;
    const SRC: u32 = 2;
    const REMAINING: u32 = 3;
    const STEP: u32 = 4;
    const DST: u32 = 5;
    const TMP: u32 = 6;

    let mut asm = Asm::new();

    get(&mut asm, PTR);
    set(&mut asm, SRC);
    get(&mut asm, LEN);
    set(&mut asm, REMAINING);

    // The validated byte count is the string's length, so allocate exactly.
    get(&mut asm, LEN);
    asm.leaf(Instruction::ArrayNewDefault(string_type.0));
    set(&mut asm, TMP);

    // Validate the whole buffer before copying any of it.
    let done = asm.label();
    let next = asm.label();
    asm.block(done);
    asm.loop_(next);

    get(&mut asm, REMAINING);
    asm.leaf(Instruction::I32Eqz);
    asm.br_if(done);

    get(&mut asm, SRC);
    get(&mut asm, REMAINING);
    asm.leaf(Instruction::Call(validate_step_index.0));
    set(&mut asm, STEP);

    // A malformed sequence reports a zero length: reject the value.
    get(&mut asm, STEP);
    asm.leaf(Instruction::I32Eqz);
    let invalid = asm.label();
    asm.if_(invalid);
    asm.leaf(Instruction::Unreachable);
    asm.end();

    get(&mut asm, SRC);
    get(&mut asm, STEP);
    asm.leaf(Instruction::I32Add);
    set(&mut asm, SRC);
    get(&mut asm, REMAINING);
    get(&mut asm, STEP);
    asm.leaf(Instruction::I32Sub);
    set(&mut asm, REMAINING);
    asm.br(next);
    asm.end();
    asm.end();

    // Copy the validated bytes unchanged into the exact-length GC array.
    constant(&mut asm, 0);
    set(&mut asm, DST);
    constant(&mut asm, 0);
    set(&mut asm, SRC);
    let copy_done = asm.label();
    let copy_next = asm.label();
    asm.block(copy_done);
    asm.loop_(copy_next);

    get(&mut asm, SRC);
    get(&mut asm, LEN);
    asm.leaf(Instruction::I32GeU);
    asm.br_if(copy_done);

    get(&mut asm, TMP);
    get(&mut asm, DST);
    get(&mut asm, PTR);
    get(&mut asm, SRC);
    asm.leaf(Instruction::I32Add);
    asm.leaf(Instruction::I32Load8U(memarg(0)));
    asm.leaf(Instruction::ArraySet(string_type.0));
    advance(&mut asm, DST, 1);
    advance(&mut asm, SRC, 1);

    asm.br(copy_next);
    asm.end();
    asm.end();

    get(&mut asm, TMP);
    asm.leaf(Instruction::RefAsNonNull);

    Function {
        symbol: crate::abi::BYTES_TO_STRING_SYMBOL,
        name: "bytes_to_string".into(),
        type_index,
        parameters: vec![ValType::I32, ValType::I32],
        locals: vec![
            ValType::I32,                     // src
            ValType::I32,                     // remaining
            ValType::I32,                     // step
            ValType::I32,                     // dst
            nullable_string_ref(string_type), // tmp
        ],
        body: asm.into_body(),
        span,
    }
}

/// The byte length of the UTF-8 sequence at `pointer`, or `0` when the bytes
/// there are not well-formed UTF-8.
///
/// Every rejection is total: an unexpected lead byte, a short buffer, a
/// non-continuation byte, an overlong form, an encoded surrogate, and a value
/// above U+10FFFF all report `0`.
pub(super) fn validate_step(type_index: TypeIndex, span: TextRange) -> Function {
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

    // ASCII: one byte
    get(&mut asm, B0);
    constant(&mut asm, 0x80);
    asm.leaf(Instruction::I32LtU);
    let ascii = asm.label();
    asm.if_(ascii);
    constant(&mut asm, 1);
    asm.leaf(Instruction::Return);
    asm.end();

    // 0x80..=0xC1 is always invalid: a bare continuation byte, or a lead below
    // the shortest two-byte form.
    get(&mut asm, B0);
    constant(&mut asm, 0xC2);
    asm.leaf(Instruction::I32LtU);
    let invalid_lead = asm.label();
    asm.if_(invalid_lead);
    constant(&mut asm, 0);
    asm.leaf(Instruction::Return);
    asm.end();

    // Two bytes: 0xC2..=0xDF
    get(&mut asm, B0);
    constant(&mut asm, 0xE0);
    asm.leaf(Instruction::I32LtU);
    let two = asm.label();
    asm.if_(two);
    // remaining < 2 is invalid
    get(&mut asm, REMAINING);
    constant(&mut asm, 2);
    asm.leaf(Instruction::I32LtU);
    let two_short = asm.label();
    asm.if_(two_short);
    constant(&mut asm, 0);
    asm.leaf(Instruction::Return);
    asm.end();
    get(&mut asm, PTR);
    asm.leaf(Instruction::I32Load8U(memarg(1)));
    set(&mut asm, B1);
    // (b1 & 0xC0) != 0x80 is invalid
    get(&mut asm, B1);
    constant(&mut asm, 0xC0);
    asm.leaf(Instruction::I32And);
    constant(&mut asm, 0x80);
    asm.leaf(Instruction::I32Ne);
    let two_bad = asm.label();
    asm.if_(two_bad);
    constant(&mut asm, 0);
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
    // A 0xC2..=0xDF lead already excludes every overlong form, so no range
    // check is needed here.
    constant(&mut asm, 2);
    asm.leaf(Instruction::Return);
    asm.end();

    // Three bytes: 0xE0..=0xEF
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
    constant(&mut asm, 0);
    asm.leaf(Instruction::Return);
    asm.end();
    get(&mut asm, PTR);
    asm.leaf(Instruction::I32Load8U(memarg(1)));
    set(&mut asm, B1);
    get(&mut asm, PTR);
    asm.leaf(Instruction::I32Load8U(memarg(2)));
    set(&mut asm, B2);
    // any non-continuation byte is invalid
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
    constant(&mut asm, 0);
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
    // overlong or an encoded surrogate is invalid
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
    constant(&mut asm, 0);
    asm.leaf(Instruction::Return);
    asm.end();
    constant(&mut asm, 3);
    asm.leaf(Instruction::Return);
    asm.end();

    // Four bytes: 0xF0..=0xF4
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
    constant(&mut asm, 0);
    asm.leaf(Instruction::Return);
    asm.end();
    // b3 is the last continuation byte; read it into `remaining`'s slot
    // through a second scratch reuse of B2's neighbour: validate directly.
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
    constant(&mut asm, 0);
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
    constant(&mut asm, 0);
    asm.leaf(Instruction::Return);
    asm.end();
    constant(&mut asm, 4);
    asm.leaf(Instruction::Return);
    asm.end();

    // Every remaining lead byte, including a lone continuation byte and 0xC0
    // or 0xC1, is invalid.
    constant(&mut asm, 0);

    Function {
        symbol: VALIDATE_STEP_SYMBOL,
        name: "validate_step".into(),
        type_index,
        parameters: vec![ValType::I32, ValType::I32],
        locals: vec![ValType::I32; 5],
        body: asm.into_body(),
        span,
    }
}

//! UTF-16 to UTF-8 encoding for a GC string at the canonical ABI boundary.

use crate::types::DefinedTypeId;
use crate::wasm::lower::asm::*;
use crate::wasm::{Function, FunctionIndex, TypeIndex};
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};

pub(super) fn string_to_bytes(
    string_type: DefinedTypeId,
    realloc_index: FunctionIndex,
    type_index: TypeIndex,
    span: TextRange,
) -> Function {
    const S: u32 = 0;
    const UNITS: u32 = 1;
    const I: u32 = 2;
    const OUT: u32 = 3;
    const PAYLOAD: u32 = 4;
    const PREFIX: u32 = 5;
    const U: u32 = 6;
    const NEXT: u32 = 7;
    const CP: u32 = 8;
    const UPPER: u32 = 9;

    let mut asm = Asm::new();

    // units = array.len(s)
    get(&mut asm, S);
    asm.leaf(Instruction::ArrayLen);
    set(&mut asm, UNITS);

    // upper = units * 3; every code unit needs at most three UTF-8 bytes.
    get(&mut asm, UNITS);
    constant(&mut asm, 3);
    asm.leaf(Instruction::I32Mul);
    set(&mut asm, UPPER);
    // An empty string still needs a valid length prefix, so never request a
    // zero-byte allocation from the allocator (which would return null).
    get(&mut asm, UPPER);
    asm.leaf(Instruction::I32Eqz);
    let empty = asm.label();
    asm.if_(empty);
    constant(&mut asm, 1);
    set(&mut asm, UPPER);
    asm.end();

    // payload = cabi_realloc(0, 0, 1, upper)
    constant(&mut asm, 0);
    constant(&mut asm, 0);
    constant(&mut asm, 1);
    get(&mut asm, UPPER);
    asm.leaf(Instruction::Call(realloc_index.0));
    set(&mut asm, PAYLOAD);

    // prefix = payload - 4
    get(&mut asm, PAYLOAD);
    constant(&mut asm, 4);
    asm.leaf(Instruction::I32Sub);
    set(&mut asm, PREFIX);

    // i = 0; out = payload
    constant(&mut asm, 0);
    set(&mut asm, I);
    get(&mut asm, PAYLOAD);
    set(&mut asm, OUT);

    let done = asm.label();
    let next = asm.label();
    asm.block(done);
    asm.loop_(next);

    get(&mut asm, I);
    get(&mut asm, UNITS);
    asm.leaf(Instruction::I32GeU);
    asm.br_if(done);

    // u = array.get_u(s, i); i += 1
    get(&mut asm, S);
    get(&mut asm, I);
    asm.leaf(Instruction::ArrayGetU(string_type.0));
    set(&mut asm, U);
    get(&mut asm, I);
    constant(&mut asm, 1);
    asm.leaf(Instruction::I32Add);
    set(&mut asm, I);

    // ASCII: u < 0x80
    get(&mut asm, U);
    constant(&mut asm, 0x80);
    asm.leaf(Instruction::I32LtU);
    let ascii = asm.label();
    asm.if_(ascii);
    get(&mut asm, OUT);
    get(&mut asm, U);
    asm.leaf(Instruction::I32Store8(memarg(0)));
    advance(&mut asm, OUT, 1);
    asm.br(next);
    asm.end();

    // Two bytes: u < 0x800
    get(&mut asm, U);
    constant(&mut asm, 0x800);
    asm.leaf(Instruction::I32LtU);
    let two = asm.label();
    asm.if_(two);
    store_derived(&mut asm, OUT, 0, 0xC0, U, 6, 0x1F);
    store_derived(&mut asm, OUT, 1, 0x80, U, 0, 0x3F);
    advance(&mut asm, OUT, 2);
    asm.br(next);
    asm.end();

    // High surrogate: 0xD800..0xDC00
    get(&mut asm, U);
    constant(&mut asm, 0xD800);
    asm.leaf(Instruction::I32GeU);
    get(&mut asm, U);
    constant(&mut asm, 0xDC00);
    asm.leaf(Instruction::I32LtU);
    asm.leaf(Instruction::I32And);
    let high = asm.label();
    asm.if_(high);
    // next = i < units ? array.get_u(s, i) : 0
    constant(&mut asm, 0);
    set(&mut asm, NEXT);
    get(&mut asm, I);
    get(&mut asm, UNITS);
    asm.leaf(Instruction::I32LtU);
    let has_next = asm.label();
    asm.if_(has_next);
    get(&mut asm, S);
    get(&mut asm, I);
    asm.leaf(Instruction::ArrayGetU(string_type.0));
    set(&mut asm, NEXT);
    asm.end();
    // Low surrogate: 0xDC00..0xE000
    get(&mut asm, NEXT);
    constant(&mut asm, 0xDC00);
    asm.leaf(Instruction::I32GeU);
    get(&mut asm, NEXT);
    constant(&mut asm, 0xE000);
    asm.leaf(Instruction::I32LtU);
    asm.leaf(Instruction::I32And);
    let paired = asm.label();
    asm.if_(paired);
    // cp = 0x10000 + ((u - 0xD800) << 10) + (next - 0xDC00)
    constant(&mut asm, 0x10000);
    get(&mut asm, U);
    constant(&mut asm, 0xD800);
    asm.leaf(Instruction::I32Sub);
    constant(&mut asm, 10);
    asm.leaf(Instruction::I32Shl);
    asm.leaf(Instruction::I32Add);
    get(&mut asm, NEXT);
    constant(&mut asm, 0xDC00);
    asm.leaf(Instruction::I32Sub);
    asm.leaf(Instruction::I32Add);
    set(&mut asm, CP);
    store_derived(&mut asm, OUT, 0, 0xF0, CP, 18, 0x07);
    store_derived(&mut asm, OUT, 1, 0x80, CP, 12, 0x3F);
    store_derived(&mut asm, OUT, 2, 0x80, CP, 6, 0x3F);
    store_derived(&mut asm, OUT, 3, 0x80, CP, 0, 0x3F);
    advance(&mut asm, OUT, 4);
    get(&mut asm, I);
    constant(&mut asm, 1);
    asm.leaf(Instruction::I32Add);
    set(&mut asm, I);
    asm.br(next);
    asm.end();
    // Unpaired high surrogate
    store_const(&mut asm, OUT, 0, 0xEF);
    store_const(&mut asm, OUT, 1, 0xBF);
    store_const(&mut asm, OUT, 2, 0xBD);
    advance(&mut asm, OUT, 3);
    asm.br(next);
    asm.end();

    // Low surrogate: 0xDC00..0xE000 (unpaired)
    get(&mut asm, U);
    constant(&mut asm, 0xDC00);
    asm.leaf(Instruction::I32GeU);
    get(&mut asm, U);
    constant(&mut asm, 0xE000);
    asm.leaf(Instruction::I32LtU);
    asm.leaf(Instruction::I32And);
    let low = asm.label();
    asm.if_(low);
    store_const(&mut asm, OUT, 0, 0xEF);
    store_const(&mut asm, OUT, 1, 0xBF);
    store_const(&mut asm, OUT, 2, 0xBD);
    advance(&mut asm, OUT, 3);
    asm.br(next);
    asm.end();

    // Three bytes: otherwise
    store_derived(&mut asm, OUT, 0, 0xE0, U, 12, 0x0F);
    store_derived(&mut asm, OUT, 1, 0x80, U, 6, 0x3F);
    store_derived(&mut asm, OUT, 2, 0x80, U, 0, 0x3F);
    advance(&mut asm, OUT, 3);
    asm.br(next);

    asm.end();
    asm.end();

    // Write the actual byte length into the allocator's length prefix.
    get(&mut asm, PREFIX);
    get(&mut asm, OUT);
    get(&mut asm, PAYLOAD);
    asm.leaf(Instruction::I32Sub);
    asm.leaf(Instruction::I32Store(memarg(0)));

    get(&mut asm, PREFIX);

    Function {
        symbol: crate::abi::STRING_TO_BYTES_SYMBOL,
        name: "string_to_bytes".into(),
        type_index,
        parameters: vec![string_ref(string_type)],
        locals: vec![ValType::I32; 9],
        body: asm.into_body(),
        span,
    }
}

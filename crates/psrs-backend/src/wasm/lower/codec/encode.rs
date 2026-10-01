//! Canonical UTF-8 byte copy out of a GC string at the ABI boundary.

use crate::types::DefinedTypeId;
use crate::wasm::lower::asm::*;
use crate::wasm::{Function, FunctionIndex, TypeIndex};
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};

/// Copies a GC string's canonical UTF-8 bytes into a transient linear buffer.
///
/// The guest string is already a byte array holding the exact UTF-8 encoding of
/// its scalar values, so this is a byte copy with no transcoding and no
/// possibility of an invalid sequence.
pub(super) fn string_to_bytes(
    string_type: DefinedTypeId,
    realloc_index: FunctionIndex,
    type_index: TypeIndex,
    span: TextRange,
) -> Function {
    const S: u32 = 0;
    const BYTES: u32 = 1;
    const I: u32 = 2;
    const OUT: u32 = 3;
    const PAYLOAD: u32 = 4;
    const PREFIX: u32 = 5;
    const UPPER: u32 = 6;

    let mut asm = Asm::new();

    // bytes = array.len(s): the stored length is the UTF-8 byte count.
    get(&mut asm, S);
    asm.leaf(Instruction::ArrayLen);
    set(&mut asm, BYTES);

    // An empty string still needs a valid length prefix, so never request a
    // zero-byte allocation from the allocator (which would return null).
    get(&mut asm, BYTES);
    set(&mut asm, UPPER);
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
    get(&mut asm, BYTES);
    asm.leaf(Instruction::I32GeU);
    asm.br_if(done);

    // Copy one byte unchanged: out[0] = s[i]; i += 1; out += 1
    get(&mut asm, OUT);
    get(&mut asm, S);
    get(&mut asm, I);
    asm.leaf(Instruction::ArrayGetU(string_type.0));
    asm.leaf(Instruction::I32Store8(memarg(0)));
    get(&mut asm, I);
    constant(&mut asm, 1);
    asm.leaf(Instruction::I32Add);
    set(&mut asm, I);
    advance(&mut asm, OUT, 1);

    asm.br(next);
    asm.end();
    asm.end();

    // Write the copied byte length into the allocator's length prefix.
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
        locals: vec![ValType::I32; 6],
        body: asm.into_body(),
        span,
    }
}

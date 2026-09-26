//! Synthesized UTF-16 <-> UTF-8 codec functions for the canonical ABI boundary.
//!
//! The component canonical ABI carries strings as UTF-8 linear bytes, while the
//! source `String` is a GC `(array (mut i16))` of UTF-16 code units. The ABI
//! adapter calls these ordinary local functions so it stays single-block.
//!
//! - `string_to_bytes` encodes a GC string into a transient linear buffer
//!   allocated by `cabi_realloc`, writes the actual byte length into the buffer's
//!   length prefix, and returns the prefix address.
//! - `bytes_to_string` decodes UTF-8 linear bytes into a fresh exact-length GC
//!   string. Invalid sequences and unpaired surrogates become U+FFFD, matching
//!   WHATWG `TextDecoder`/`TextEncoder`.
//! - `decode_step` decodes one UTF-8 sequence, returning a packed
//!   `(code_point << 6) | (utf16_units << 3) | consumed_bytes`.

mod decode;
mod encode;

#[cfg(test)]
mod tests;

use crate::types::DefinedTypeId;
use crate::wasm::{FuncType, Function, FunctionIndex, TypeIndex};
use psrs_span::TextRange;
use wasm_encoder::ValType;

/// The three function types used by the codec.
pub(super) fn signatures(string_ref: ValType) -> (FuncType, FuncType, FuncType) {
    (
        FuncType {
            parameters: vec![string_ref],
            results: vec![ValType::I32],
        },
        FuncType {
            parameters: vec![ValType::I32, ValType::I32],
            results: vec![string_ref],
        },
        FuncType {
            parameters: vec![ValType::I32, ValType::I32],
            results: vec![ValType::I32],
        },
    )
}

/// Builds `string_to_bytes`, `bytes_to_string`, and `decode_step` in that order.
pub(super) fn synthesize(
    string_type: DefinedTypeId,
    realloc_index: FunctionIndex,
    decode_step_index: FunctionIndex,
    string_to_bytes_type: TypeIndex,
    bytes_to_string_type: TypeIndex,
    decode_step_type: TypeIndex,
    span: TextRange,
) -> Vec<Function> {
    vec![
        encode::string_to_bytes(string_type, realloc_index, string_to_bytes_type, span),
        decode::bytes_to_string(string_type, decode_step_index, bytes_to_string_type, span),
        decode::decode_step(decode_step_type, span),
    ]
}

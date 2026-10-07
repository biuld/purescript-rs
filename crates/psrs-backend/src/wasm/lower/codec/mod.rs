//! Synthesized string boundary helpers for the canonical ABI.
//!
//! The component canonical ABI carries a WIT `string` as UTF-8 linear bytes, and
//! a guest source `String` is a GC array holding the canonical UTF-8 bytes of
//! its Unicode scalar values. The boundary therefore copies bytes and never
//! transcodes. The ABI adapter calls these ordinary local functions so it stays
//! single-block.
//!
//! - `string_to_bytes` copies a GC string's bytes into a transient linear
//!   buffer allocated by `cabi_realloc`, writes the byte length into the
//!   buffer's length prefix, and returns the prefix address.
//! - `bytes_to_string` strictly validates incoming bytes, rejecting malformed
//!   text, and copies the valid bytes unchanged into a fresh exact-length GC
//!   string.
//! - `validate_step` returns the byte length of the UTF-8 sequence at a
//!   pointer, or `0` when the bytes there are not well-formed UTF-8.
//!
//! A WIT `list<u8>` is not this path: it is `Array Int` and is copied as
//! uninterpreted bytes by the element-wise list lowering.

mod decode;
mod encode;

#[cfg(test)]
mod tests;

use crate::types::DefinedTypeId;
use crate::wasm::{FuncType, Function, FunctionIndex, TypeIndex};
use psrs_span::TextRange;

/// The three function types used by the boundary helpers.
pub(super) fn signatures(string: DefinedTypeId) -> (FuncType, FuncType, FuncType) {
    use crate::abi;
    (
        super::generated_signature(abi::STRING_TO_BYTES_SYMBOL, Some(string)),
        super::generated_signature(abi::BYTES_TO_STRING_SYMBOL, Some(string)),
        super::generated_signature(abi::VALIDATE_STEP_SYMBOL, Some(string)),
    )
}

/// Builds `string_to_bytes`, `bytes_to_string`, and `validate_step` in that order.
pub(super) fn synthesize(
    string_type: DefinedTypeId,
    realloc_index: FunctionIndex,
    validate_step_index: FunctionIndex,
    string_to_bytes_type: TypeIndex,
    bytes_to_string_type: TypeIndex,
    validate_step_type: TypeIndex,
    span: TextRange,
) -> Vec<Function> {
    vec![
        encode::string_to_bytes(string_type, realloc_index, string_to_bytes_type, span),
        decode::bytes_to_string(string_type, validate_step_index, bytes_to_string_type, span),
        decode::validate_step(validate_step_type, span),
    ]
}

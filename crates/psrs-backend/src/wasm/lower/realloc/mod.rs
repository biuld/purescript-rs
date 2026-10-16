//! Local forwarder for the allocator runtime unit's `cabi_realloc`.
//!
//! Codecs, cleanup, and the component host call this function. It forwards the
//! canonical arguments to the imported provider and returns that provider's
//! pointer. The provider owns allocation, freeing, and resizing.

use super::super::{Function, Op, TypeIndex};
use crate::abi;
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};

/// Builds the forwarding `cabi_realloc`.
///
/// `import` is the core function index of the allocator unit's export.
pub(super) fn forward_realloc(type_index: TypeIndex, import: u32, span: TextRange) -> Function {
    Function {
        symbol: abi::REALLOC_SYMBOL,
        name: psrs_runtime::REALLOC_EXPORT.into(),
        type_index,
        parameters: vec![ValType::I32; 4],
        locals: Vec::new(),
        body: vec![
            Op::Leaf(Instruction::LocalGet(0)),
            Op::Leaf(Instruction::LocalGet(1)),
            Op::Leaf(Instruction::LocalGet(2)),
            Op::Leaf(Instruction::LocalGet(3)),
            Op::Leaf(Instruction::Call(import)),
        ],
        span,
    }
}

/// Returns the checked application's constant boundary without using linear memory.
pub(super) fn heap_getter(type_index: TypeIndex, heap_start: u32, span: TextRange) -> Function {
    Function {
        symbol: abi::HEAP_BASE_SYMBOL,
        name: psrs_runtime::HEAP_BOUNDARY_IMPORT.into(),
        type_index,
        parameters: Vec::new(),
        locals: Vec::new(),
        body: vec![Op::Leaf(Instruction::I32Const(heap_start as i32))],
        span,
    }
}

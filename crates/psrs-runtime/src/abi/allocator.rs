//! Canonical allocator bindings, raw signature, and private storage.

use super::{RawCallProtocol, RawFunctionAbi, RawType};

/// Private core-module import identity of the canonical allocator.
pub const ALLOCATOR_MODULE: &str = "psrs:allocator";
/// Exported canonical realloc function.
pub const REALLOC_EXPORT: &str = "cabi_realloc";
/// Heap-boundary getter imported from the application.
pub const HEAP_BOUNDARY_IMPORT: &str = "get_heap_base";
/// First byte of the allocator artifact's static data.
pub const ALLOCATOR_STATIC_START: u32 = 32768;
/// Lowest permitted allocator stack address; checked frames stay above scratch.
pub const ALLOCATOR_STACK_BOTTOM: u32 = 16384;
/// Initial stack pointer and end of the allocator stack reservation.
pub const ALLOCATOR_STACK_TOP: u32 = 32768;
/// Exclusive end of the allocator static data reservation.
pub const ALLOCATOR_STATIC_END: u32 = 65536;
/// Contract heap boundary when no later reservation extends it.
pub const ALLOCATOR_HEAP_START: u32 = 65536;

pub const ALLOCATOR_REALLOC: RawFunctionAbi = RawFunctionAbi {
    export: REALLOC_EXPORT,
    parameters: &[RawType::I32, RawType::I32, RawType::I32, RawType::I32],
    result: Some(RawType::I32),
    protocol: RawCallProtocol::Scalars,
};

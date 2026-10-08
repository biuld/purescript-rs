//! Symbols reserved from the high end of the intrinsic-symbol space.
//!
//! Allocators that count downward from `u32::MAX` must skip this set. The
//! numeric-runtime symbols are ordinary MIR imports of artifact exports. The
//! allocator and string-boundary symbols are synthesized locally and are never
//! core imports.

use psrs_hir::{ModuleId, SymbolId};

/// Reserved MIR symbol for the allocator synthesized after ABI memory layout
/// is known. Calls to this symbol become calls to the local `cabi_realloc`
/// function during Wasm lowering; it is never emitted as a core import.
pub(crate) const REALLOC_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 1);

/// Reserved symbols for the string boundary helpers at the canonical ABI.
/// P10 synthesizes them as ordinary local Wasm functions; like
/// `REALLOC_SYMBOL` they are never emitted as core imports.
pub(crate) const STRING_TO_BYTES_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 2);
pub(crate) const BYTES_TO_STRING_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 3);

/// Reserved MIR symbol for the synthesized `validate_step` helper. Like the
/// other boundary symbols it is never a core import.
pub(crate) const VALIDATE_STEP_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 4);

pub(crate) const NUMBER_TO_STRING_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 5);
pub(crate) const NUMBER_FROM_DECIMAL_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 6);
pub(crate) const NUMBER_ACOS_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 7);
pub(crate) const NUMBER_ASIN_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 8);
pub(crate) const NUMBER_ATAN_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 9);
pub(crate) const NUMBER_ATAN2_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 10);
pub(crate) const NUMBER_SIN_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 11);
pub(crate) const NUMBER_COS_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 12);
pub(crate) const NUMBER_TAN_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 13);
pub(crate) const NUMBER_EXP_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 14);
pub(crate) const NUMBER_LOG_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 15);
pub(crate) const NUMBER_POW_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 16);
pub(crate) const NUMBER_MIN_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 17);
pub(crate) const NUMBER_MAX_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 18);
pub(crate) const NUMBER_SIGN_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 19);
pub(crate) const NUMBER_REMAINDER_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 20);
pub(crate) const NUMBER_IS_NAN_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 21);
pub(crate) const NUMBER_NAN_SYMBOL: SymbolId = SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 22);
pub(crate) const NUMBER_INFINITY_SYMBOL: SymbolId =
    SymbolId::new(ModuleId::INTRINSICS, u32::MAX - 23);

/// Every symbol this allocator must not reuse.
pub(crate) const RESERVED_ABI_SYMBOLS: [SymbolId; 23] = [
    REALLOC_SYMBOL,
    STRING_TO_BYTES_SYMBOL,
    BYTES_TO_STRING_SYMBOL,
    VALIDATE_STEP_SYMBOL,
    NUMBER_TO_STRING_SYMBOL,
    NUMBER_FROM_DECIMAL_SYMBOL,
    NUMBER_ACOS_SYMBOL,
    NUMBER_ASIN_SYMBOL,
    NUMBER_ATAN_SYMBOL,
    NUMBER_ATAN2_SYMBOL,
    NUMBER_SIN_SYMBOL,
    NUMBER_COS_SYMBOL,
    NUMBER_TAN_SYMBOL,
    NUMBER_EXP_SYMBOL,
    NUMBER_LOG_SYMBOL,
    NUMBER_POW_SYMBOL,
    NUMBER_MIN_SYMBOL,
    NUMBER_MAX_SYMBOL,
    NUMBER_SIGN_SYMBOL,
    NUMBER_REMAINDER_SYMBOL,
    NUMBER_IS_NAN_SYMBOL,
    NUMBER_NAN_SYMBOL,
    NUMBER_INFINITY_SYMBOL,
];

//! The target runtime package.
//!
//! This crate has two distinct build entry points:
//!
//! - The [`catalog`] module (feature `catalog`) exposes immutable, host-consumed
//!   metadata: pinned WIT source bytes, the default command-world identity, and
//!   the compiler-owned formatter artifact with its provenance and storage
//!   contract. It links no executable target code.
//! - Feature `formatter` compiles the numeric formatting, complete-decimal
//!   conversion, and inverse trigonometric exports, built for `wasm32-unknown-unknown`
//!   and embedded as the pinned artifact. It embeds neither WIT text nor the
//!   catalog.
//!
//! The compiler depends on this crate with `default-features = false,
//! features = ["catalog"]`; the Wasm artifact build uses
//! `--no-default-features --features formatter`.

#![cfg_attr(all(feature = "formatter", target_arch = "wasm32"), no_std)]

#[cfg(feature = "catalog")]
pub mod catalog;
#[cfg(feature = "catalog")]
pub use catalog::*;

#[cfg(feature = "formatter")]
mod acos;
#[cfg(feature = "formatter")]
mod asin;
#[cfg(feature = "formatter")]
mod atan;
#[cfg(feature = "formatter")]
mod decimal;
#[cfg(feature = "formatter")]
mod formatter;
#[cfg(feature = "formatter")]
pub use acos::number_acos;
#[cfg(feature = "formatter")]
pub use asin::number_asin;
#[cfg(feature = "formatter")]
pub use atan::number_atan;
#[cfg(feature = "formatter")]
pub use decimal::number_from_decimal;
#[cfg(feature = "formatter")]
pub use formatter::number_to_string;

/// Maximum capacity required by an ECMAScript binary64 token.
pub const NUMBER_CAPACITY: usize = 32;
/// Private core-module import identity used by the compiler.
pub const MODULE_NAME: &str = "psrs:runtime";
/// Exported raw formatting function.
pub const NUMBER_EXPORT: &str = "number_to_string";
/// Exported raw complete-decimal conversion function.
pub const DECIMAL_EXPORT: &str = "number_from_decimal";
/// Exported raw inverse-cosine function.
pub const ACOS_EXPORT: &str = "number_acos";
/// Exported raw inverse-sine function.
pub const ASIN_EXPORT: &str = "number_asin";
/// Exported raw inverse-tangent function.
pub const ATAN_EXPORT: &str = "number_atan";
/// Lower addresses remain owned by the application's canonical ABI.
pub const RESERVED_START: u32 = 65536;
/// Static data must end before the separately reserved 64 KiB stack.
pub const STACK_BOTTOM: u32 = 131072;
/// The caller's allocator begins after the private stack.
pub const HEAP_START: u32 = 196608;
/// The runtime imports the application's canonical memory.
pub const MEMORY_MODULE: &str = "env";
/// The runtime's memory import field.
pub const MEMORY_FIELD: &str = "memory";
/// The runtime exports its heap boundary for the preparer to relocate.
pub const HEAP_BASE_EXPORT: &str = "__heap_base";

/// Raw scalar types in the runtime's core-Wasm function ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawType {
    I32,
    I64,
    F32,
    F64,
}

/// Language-value transport and normal-return ownership for a raw export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawCallProtocol {
    /// Scalar arguments/results only; Unit results use a void export.
    Scalars,
    /// Borrowed canonical UTF-8 bytes; caller copies, then releases on return.
    Utf8Input,
    /// Caller-owned bounded UTF-8 output; caller recovers and releases on return.
    Utf8Output { capacity: usize },
}

/// An artifact export's raw signature and language-value transport protocol.
pub struct RawFunctionAbi {
    pub export: &'static str,
    pub parameters: &'static [RawType],
    pub result: Option<RawType>,
    pub protocol: RawCallProtocol,
}

pub const NUMBER_FORMAT: RawFunctionAbi = RawFunctionAbi {
    export: NUMBER_EXPORT,
    parameters: &[RawType::F64, RawType::I32, RawType::I32],
    result: Some(RawType::I32),
    protocol: RawCallProtocol::Utf8Output {
        capacity: NUMBER_CAPACITY,
    },
};

pub const NUMBER_PARSE: RawFunctionAbi = RawFunctionAbi {
    export: DECIMAL_EXPORT,
    parameters: &[RawType::I32, RawType::I32],
    result: Some(RawType::F64),
    protocol: RawCallProtocol::Utf8Input,
};

pub const NUMBER_ACOS: RawFunctionAbi = RawFunctionAbi {
    export: ACOS_EXPORT,
    parameters: &[RawType::F64],
    result: Some(RawType::F64),
    protocol: RawCallProtocol::Scalars,
};

pub const NUMBER_ASIN: RawFunctionAbi = RawFunctionAbi {
    export: ASIN_EXPORT,
    parameters: &[RawType::F64],
    result: Some(RawType::F64),
    protocol: RawCallProtocol::Scalars,
};

pub const NUMBER_ATAN: RawFunctionAbi = RawFunctionAbi {
    export: ATAN_EXPORT,
    parameters: &[RawType::F64],
    result: Some(RawType::F64),
    protocol: RawCallProtocol::Scalars,
};

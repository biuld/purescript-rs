//! Raw ABI contracts shared by target implementations and the host catalog.
//! This module contains data only and embeds no artifact or WIT bytes.

mod allocator;
mod number;
mod storage;

pub use allocator::*;
pub use number::*;
pub use storage::*;

/// The runtime imports the application's canonical memory.
pub const MEMORY_MODULE: &str = "env";
/// The runtime's memory import field.
pub const MEMORY_FIELD: &str = "memory";
/// Core import namespace used by composition for application exports.
pub const APPLICATION_MODULE: &str = "__main_module__";

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

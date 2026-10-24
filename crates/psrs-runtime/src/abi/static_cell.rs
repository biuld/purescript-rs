//! Per-binding static storage, generated in the application instance.
//! The source binding must be closed and monomorphic before erasure. Its
//! resolved declaration identity selects one cell; there is no dynamic key.
use super::{StorageSourceContract, StorageValue};
pub const STATIC_CELL_MODULE: &str = "psrs:runtime-static";
pub const STATIC_CELL_EXPORT: &str = "cell";
pub const STATIC_CELL_SOURCE: StorageSourceContract = StorageSourceContract {
    parameters: &[StorageValue::Element],
    payload: StorageValue::Array,
};

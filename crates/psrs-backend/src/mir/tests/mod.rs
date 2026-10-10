//! MIR-level tests grouped by the boundary each one exercises.
use psrs_span::TextRange;

mod lowering;
mod runtime;
mod verify;

pub(super) fn span() -> TextRange {
    TextRange::new(0, 1)
}

pub(super) fn registry() -> crate::abi::WasiRegistry {
    crate::abi::WasiRegistry::load().expect("the vendored WASI WIT should load")
}

//! Canonical allocation: portable validation and bookkeeping, plus a Wasm adapter.

mod algorithm;
mod phase;
mod provision;

#[cfg(target_arch = "wasm32")]
mod wasm;

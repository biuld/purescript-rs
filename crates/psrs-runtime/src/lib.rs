//! One runtime package with separate ABI, catalog, and executable module owners.
//!
//! `abi` defines data-only raw contracts for every build. Feature `catalog`
//! exposes pinned WIT, artifact metadata, and package units to native compiler
//! consumers and is the only default feature. Feature `number` builds the numeric implementation;
//! feature `allocator` builds the canonical allocator. Features are additive;
//! each pinned Wasm artifact selects exactly one executable feature.
//! Feature `gc-storage` exposes a host encoder for an independent GC storage
//! module; its target operations allocate no Rust or linear-memory heap.
//! Feature `artifact-tool` builds the host-only `psrs-runtime-package` binary.

#![cfg_attr(target_arch = "wasm32", no_std)]

pub mod abi;
pub use abi::*;

#[cfg(all(feature = "gc-storage", not(target_arch = "wasm32")))]
pub mod storage;

#[cfg(all(feature = "catalog", not(target_arch = "wasm32")))]
pub mod catalog;
#[cfg(all(feature = "catalog", not(target_arch = "wasm32")))]
pub use catalog::*;

#[cfg(feature = "number")]
mod number;
#[cfg(feature = "number")]
pub use number::*;

#[cfg(all(feature = "allocator", any(test, target_arch = "wasm32")))]
mod allocator;

#[cfg(all(target_arch = "wasm32", not(test)))]
mod panic;

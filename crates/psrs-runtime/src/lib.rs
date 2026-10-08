//! One runtime package with separate ABI, catalog, and executable module owners.
//!
//! `abi` defines data-only raw contracts for every build. Feature `catalog`
//! exposes pinned WIT, artifact metadata, and package units to native compiler
//! consumers. Feature `formatter` builds the numeric implementation in `number`;
//! feature `allocator` builds the canonical allocator. Each Wasm artifact enables
//! exactly one executable feature and embeds no catalog assets.

#![cfg_attr(
    all(
        any(feature = "formatter", feature = "allocator"),
        target_arch = "wasm32"
    ),
    no_std
)]

pub mod abi;
pub use abi::*;

#[cfg(feature = "catalog")]
pub mod catalog;
#[cfg(feature = "catalog")]
pub use catalog::*;

#[cfg(feature = "formatter")]
mod number;
#[cfg(feature = "formatter")]
pub use number::*;

#[cfg(all(feature = "allocator", any(test, target_arch = "wasm32")))]
mod allocator;

#[cfg(all(
    any(feature = "formatter", feature = "allocator"),
    target_arch = "wasm32"
))]
mod panic;

#[cfg(all(feature = "formatter", feature = "allocator", target_arch = "wasm32"))]
compile_error!("the formatter and allocator are separate wasm32 artifacts");

//! Immutable host-consumed runtime catalog. No executable target code is built here.
//! ABI contracts live in `abi`; each unit declares its artifact and operations
//! together, while WIT sources and the package aggregate have separate owners.

mod allocator;
mod model;
mod number;
mod package;
mod wit;

pub use allocator::*;
pub use model::*;
pub use number::*;
pub use package::*;
pub use wit::*;

const VERSION: &str = "1";

const fn op(name: &'static str, abi: &'static crate::abi::RawFunctionAbi) -> ProvidedOperation {
    ProvidedOperation {
        name,
        version: VERSION,
        abi,
    }
}

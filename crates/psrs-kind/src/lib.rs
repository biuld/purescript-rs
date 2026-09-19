//! Kind checking for a resolved module.
//!
//! This pass consumes [`psrs_hir::Module`] and reports kind errors aligned to
//! official PureScript `errorCode`s. Kinds are checked before THIR
//! construction, so no kind information escapes into a long-lived IR.

mod check;
mod kind;

pub use check::check_module;
pub use kind::{Kind, KindDiagnostic, KindScheme};

#[cfg(test)]
mod tests;

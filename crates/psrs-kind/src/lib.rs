//! Program-level kind checking.
//!
//! Kinds are PureScript types, so one denotation, one primitive kind table, and
//! one solver own every kind in the compiler: a declaration annotation, an
//! instance head, and the kind of every type unknown all resolve through
//! [`denote_kind`], [`primitive_kind`], and [`unify_kind`]. The environment
//! those operations produce is program-level, and an imported declaration's
//! kind is the one its declaring module checked.
//!
//! This pass consumes [`psrs_hir::Module`] and reports kind errors aligned to
//! official PureScript `errorCode`s, each attributed to the module that
//! declares the offending type. Kinds are checked before THIR construction, so
//! no kind information escapes into a long-lived IR.

mod check;
mod denote;
mod kind;
mod solve;

pub use check::{check_module, check_program};
pub use denote::{KindScope, denote_kind};
pub use kind::{
    CheckedKindEnv, Kind, KindDiagnostic, KindScheme, constraint_kind, primitive_kind, type_kind,
};
pub use solve::{KindError, KindState, bind_kind_variable, occurs, substitute, unify_kind};

#[cfg(test)]
mod tests;

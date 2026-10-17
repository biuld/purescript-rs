//! The one owner of pattern-matrix usefulness analysis.
//!
//! Coverage and first-match compilation agree on one semantic question — what
//! a set of source alternatives cannot cover — and the type checker's `Partial`
//! exhaustiveness authorization and the decision compiler's first-match
//! lowering both consume this module's answer. The engine is typed over a
//! neutral pattern algebra and a [`Types`] abstraction, so neither the type
//! checker nor the backend reconstructs the analysis for itself.
//!
//! `Partial`'s exhaustiveness payload (the official witness behind
//! `addPartialConstraint`) comes from `Analysis::witness_atoms`; every other
//! member stays erasure-neutral.

mod analyze;
mod engine;
mod pattern;
mod render;
mod types;

#[cfg(test)]
mod tests;

pub use analyze::{Analysis, Branch, analyze};
// `Ty` remains the supplying stage's own type identity; nothing here fixes one.
pub use pattern::{Literal, Pattern};
pub use types::{ConstructorRow, Types};

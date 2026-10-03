//! Constraint solving: the wanted-solver entry, the search over givens, superclass
//! paths, and instances, and the builders a selected solution needs.
//!
//! The module is split because the whole exceeds the repository's source layout
//! limit in one file. `entry` owns the entry and the retention policy, `search`
//! owns the search itself, and `builders` owns what a selected solution implies.

mod builders;
mod entry;
mod search;

pub(in crate::typecheck) use entry::{SolveDepth, UnsolvedPolicy};

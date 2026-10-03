//! Class and instance elaboration for the type checker.
//!
//! `environment` validates class and instance declarations and records the
//! searchable class environment. `evidence` elaborates constraints into THIR
//! dictionary evidence (givens, instance dictionaries, superclass projections,
//! and method selection); `solve` discharges wanted constraints and owns the
//! single dispatch site, where givens, the primitive rule table, and instance
//! search are consulted in that order; `instance` builds instance dictionary
//! values. The `Prim` rules themselves live in [`prim`](super::prim).

mod coherence;
mod deriving;
mod environment;
mod evidence;
mod fundeps;
mod instance;
mod locals;
mod matching;
mod solve;
mod superclass;

pub(in crate::typecheck) use fundeps::collect_infer_variables;
pub(in crate::typecheck) use locals::next_local_id;
pub(in crate::typecheck) use solve::SolveDepth;

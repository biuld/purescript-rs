//! Class and instance elaboration for the type checker.
//!
//! `environment` validates class and instance declarations and records the
//! searchable class environment. `evidence` elaborates constraints into THIR
//! dictionary evidence (givens, instance dictionaries, superclass projections,
//! and method selection); `solve` discharges wanted constraints; `instance`
//! builds instance dictionary values.

mod coercion;
mod coherence;
mod environment;
mod evidence;
mod fundeps;
mod instance;
mod locals;
mod matching;
mod solve;

pub(in crate::typecheck) use locals::next_local_id;

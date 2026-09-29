//! Class and instance elaboration for the type checker.
//!
//! `environment` validates class and instance declarations and records the
//! searchable class environment. `evidence` elaborates constraints into THIR
//! dictionary evidence (givens, instance dictionaries, and method selection).

mod environment;
mod evidence;

pub(in crate::typecheck) use environment::next_local_id;

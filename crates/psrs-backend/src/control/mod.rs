//! Stage-independent contracts for the control machine.
//!
//! `schema` names the machine's cases, fields and roles; `dispatch` names the
//! required alternatives of each carrier and verifies that a dispatch covers
//! them; `provenance` states the ownership relations that CC and MIR control
//! checks share. All are independent of any IR shape.
pub(crate) mod schema;

pub(crate) mod dispatch;
pub(crate) mod provenance;
pub(crate) mod steps;

pub(crate) mod execution;

pub(crate) mod owned;

pub(crate) mod events;
pub(crate) mod lifecycle;

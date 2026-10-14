//! Stage-independent contracts for the control machine.
//!
//! `dispatch` names the required alternatives of each carrier and verifies that
//! a dispatch covers them; `provenance` states the ownership relations that CC
//! and MIR control checks share. Both are independent of any IR shape.
pub(crate) mod dispatch;
pub(crate) mod provenance;
pub(crate) mod steps;

pub(crate) mod execution;

pub(crate) mod owned;

pub(crate) mod events;
pub(crate) mod lifecycle;

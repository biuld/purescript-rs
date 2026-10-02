//! The official-suite scoreboards, one submodule per gate.
//!
//! Each test is `#[ignore]`d because it needs the vendored corpus, and each one
//! prints its own report rather than asserting a fixed number: the number moves
//! as the compiler and the standard library grow, so a regression is a change
//! in the printed report, not a failing constant.
//!
//! * [`parse`] measures L1 parse agreement against the oracle.
//! * [`resolve`] measures L2 resolution agreement.
//! * [`kinds`] measures L3 kind agreement.
//! * [`types`] measures L4 type and L5 class agreement in one pass.
//! * [`runtime`] measures L6/M7 compile, validate, and run agreement.
//!
//! The shared corpus helpers live in [`corpus`].

mod corpus;
mod kinds;
mod parse;
mod resolve;
mod runtime;
mod types;

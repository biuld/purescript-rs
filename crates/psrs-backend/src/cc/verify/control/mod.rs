//! Control helper body checks at the CC verification boundary.

use crate::BackendError;
use crate::cc::{Function, RepresentationTable};
use psrs_hir::SymbolId;
use std::collections::HashMap;

mod checkpoints;
mod clauses;
mod context_entry;
mod dispatch;
mod execution;
mod exits;
mod finish_consumption;
mod finish_creation;
mod initialization;
mod lifecycle;
mod owned;
mod refinements;
mod replay;
mod root;
mod steps;
mod traces;

pub(super) fn verify_before_assignments(
    function: &Function,
    representations: &RepresentationTable,
) -> Result<(), Vec<BackendError>> {
    finish_consumption::verify(function, representations)
}

pub(super) fn verify_after_assignments(
    function: &Function,
    representations: &RepresentationTable,
    functions: Option<&HashMap<SymbolId, &Function>>,
) -> Result<(), Vec<BackendError>> {
    dispatch::verify(function, representations)?;
    exits::verify(function, representations)?;
    clauses::verify(function, representations)?;
    root::verify(function, representations)?;
    checkpoints::verify(function, representations)?;
    finish_creation::verify(function, representations)?;
    context_entry::verify(function, representations)?;
    replay::verify(function, representations, functions)?;
    steps::verify(function, representations)?;
    execution::verify(function, representations)?;
    owned::verify(function, representations)?;
    execution::verify_effects(function, representations, functions)?;
    lifecycle::verify(function, representations)?;
    initialization::verify(function, representations, functions)
}

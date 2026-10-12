//! Control helper body checks at the CC verification boundary.

use crate::BackendError;
use crate::cc::{Function, RepresentationTable};
use psrs_hir::SymbolId;
use std::collections::HashMap;

mod checkpoints;
mod context_entry;
mod finish_consumption;
mod finish_creation;
mod replay;
mod root;

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
    root::verify(function, representations)?;
    checkpoints::verify(function, representations)?;
    finish_creation::verify(function, representations)?;
    context_entry::verify(function, representations)?;
    replay::verify(function, representations, functions)
}

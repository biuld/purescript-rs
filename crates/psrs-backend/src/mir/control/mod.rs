//! Control evidence and helper body contracts after physical layout.

use crate::BackendError;
use crate::mir::Module;

mod checkpoints;
mod clauses;
mod context_entry;
mod contracts;
mod dispatch;
mod execution;
mod exits;
mod finish_consumption;
mod finish_creation;
mod initialization;
mod lifecycle;
mod owned;
mod publication;
mod refinements;
mod replay;
mod root;
mod steps;
mod traces;

pub(super) use contracts::verify_helper_calls;

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    contracts::verify(module, errors);
    dispatch::verify(module, errors);
    exits::verify(module, errors);
    clauses::verify(module, errors);

    root::verify(module, errors);
    finish_consumption::verify(module, errors);
    checkpoints::verify(module, errors);
    replay::verify(module, errors);
    finish_creation::verify(module, errors);
    context_entry::verify(module, errors);
    steps::verify(module, errors);
    execution::verify(module, errors);
    owned::verify(module, errors);
    lifecycle::verify(module, errors);
    initialization::verify(module, errors);
    if let Err(mut failures) = publication::verify(module) {
        errors.append(&mut failures);
    }
}

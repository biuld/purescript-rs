//! Control evidence and helper body contracts after physical layout.

use crate::BackendError;
use crate::mir::Module;

mod checkpoints;
mod contracts;
mod finish_consumption;
mod finish_creation;
mod replay;

pub(super) use contracts::verify_helper_calls;

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    contracts::verify(module, errors);
    finish_consumption::verify(module, errors);
    checkpoints::verify(module, errors);
    replay::verify(module, errors);
    finish_creation::verify(module, errors);
}

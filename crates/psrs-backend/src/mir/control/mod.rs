//! Control helper call contracts after physical layout.
//!
//! The machine recognizers are removed: construction now owns the machine
//! semantics (see DEC-21 and `cc/lower/control/plan`). Only the helper call
//! binding contract remains, so a lowered call still names a checked helper.

use crate::BackendError;
use crate::mir::Module;

mod boundary;
mod contracts;

pub(super) use contracts::verify_helper_calls;

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    boundary::verify(module, errors);
}

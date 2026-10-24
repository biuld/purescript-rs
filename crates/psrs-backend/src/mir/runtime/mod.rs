//! Source-anchored external calls using checked physical binding signatures.
use super::{Import, layout::PlannedLayout, reachable::ReachableHandles};
use crate::bindings::plan_runtime_import as plan_import;
use crate::{BackendError, RuntimeBinding, cc};

mod context;
mod static_cell;
pub(in crate::mir) use context::RuntimeContext;

#[derive(Clone, Debug, PartialEq, Eq)]
/// External provider/export identity retained after ABI lowering.
pub struct RuntimeImport {
    pub module: String,
    pub function: String,
}

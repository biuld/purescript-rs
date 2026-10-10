//! Source-anchored external calls using checked physical binding signatures.
use super::{Import, layout::PlannedLayout, reachable::ReachableHandles};
use crate::bindings::{
    RawCallResult, plan_runtime_import as plan_import, runtime_projection_error as error,
    verify_runtime_import as verify_import,
};
use crate::types::{HeapType, RefType, ValueType};
use crate::{BackendError, RuntimeBinding, cc};

mod context;
mod invocation;
pub(in crate::mir) use context::RuntimeContext;
pub(super) use invocation::RawInvocation;

#[derive(Clone, Debug, PartialEq, Eq)]
/// External provider/export identity retained after ABI lowering.
pub struct RuntimeImport {
    pub module: String,
    pub function: String,
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod function_tests;

//! P9 representation-planner contract and the Wasm GC planner.
//!
//! CC carries target-neutral representation requirements; P9 realizes them as a
//! concrete layout. Under `DEC-09`, Wasm GC is the only language-heap planner.

use super::layout::{LayoutError, PlannedLayout};
use crate::TargetCapabilities;
use crate::cc::Module as CcModule;

/// A P9 planner consumes target-neutral CC requirements and produces a
/// target-specific layout description. CC never depends on this trait or on a
/// concrete planner.
pub(super) trait RepresentationPlanner {
    type Layout;

    fn plan_module(&self, module: &CcModule) -> Result<Self::Layout, LayoutError>;
}

/// The Wasm GC implementation of the planner contract.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct GcPlanner {
    pub(super) target: TargetCapabilities,
}

impl RepresentationPlanner for GcPlanner {
    type Layout = PlannedLayout;

    fn plan_module(&self, module: &CcModule) -> Result<Self::Layout, LayoutError> {
        PlannedLayout::plan_module(module, self.target)
    }
}

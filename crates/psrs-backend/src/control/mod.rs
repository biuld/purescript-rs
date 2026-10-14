//! Stage-independent contracts for the control machine.
//!
//! The machine semantics live at the checked construction plan
//! (`cc/lower/control/plan`). This module keeps the named schema the plan,
//! layout and verification consume, plus the flat-driver action inventory.
pub(crate) mod execution;
pub(crate) mod schema;

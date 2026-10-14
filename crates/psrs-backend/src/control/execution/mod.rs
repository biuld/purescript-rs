//! The named action inventory of the flat driver.
//!
//! Every selection the operational semantics admits has exactly one entry. The
//! checked construction plan records them and `plan::check` requires the whole
//! inventory, so a missing case is a construction failure rather than a silent
//! gap.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ActionId {
    PerformRoot,
    ReturnEmptyRoot,
    ReturnCons,
    ReturnFinish,
    AppendEmpty,
    AppendCons,
    AppendAppend,
    AppendFinish,
}

impl ActionId {
    /// The complete required action inventory of the flat driver.
    pub(crate) const ALL: [Self; 8] = [
        Self::PerformRoot,
        Self::ReturnEmptyRoot,
        Self::ReturnCons,
        Self::ReturnFinish,
        Self::AppendEmpty,
        Self::AppendCons,
        Self::AppendAppend,
        Self::AppendFinish,
    ];
}

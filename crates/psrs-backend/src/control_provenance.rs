//! Stage-independent ownership relations for the control machine.
use crate::types::ValueId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FinishProjection {
    pub owner: ValueId,
    pub field: u32,
}

/// Finishing restores all lifecycle state from one immutable finish record.
/// A shape-compatible parent or a projection from another record is insufficient.
pub(crate) fn finish_relation(
    extent: Option<FinishProjection>,
    queue: Option<FinishProjection>,
    parent: Option<FinishProjection>,
) -> Result<(), &'static str> {
    let extent = extent
        .filter(|projection| projection.field == 0)
        .ok_or("control finish must close its frame's extent, not another boundary")?;
    if queue
        != Some(FinishProjection {
            owner: extent.owner,
            field: 1,
        })
        || parent
            != Some(FinishProjection {
                owner: extent.owner,
                field: 2,
            })
    {
        return Err("control finish must restore the queue and parent from the same frame");
    }
    Ok(())
}

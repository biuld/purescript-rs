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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CaptureProjection {
    pub receiver: ValueId,
    pub slot: u32,
}

pub(crate) fn checkpoint_capture_relation(
    receiver: ValueId,
    queue: Option<CaptureProjection>,
    checkpoint: Option<CaptureProjection>,
) -> Result<(), &'static str> {
    if queue != Some(CaptureProjection { receiver, slot: 0 })
        || checkpoint != Some(CaptureProjection { receiver, slot: 1 })
    {
        return Err("raw resumption must save its own queue and checkpoint capture slots");
    }
    Ok(())
}

pub(crate) fn dispatch_checkpoint_relation(
    context: ValueId,
    projection: Option<(ValueId, u32, u32)>,
) -> Result<(), &'static str> {
    match projection {
        Some((owner, 0, 0) | (owner, 1, 5)) if owner == context => Ok(()),
        _ => Err("raw resumption must capture its current dispatch context checkpoint"),
    }
}

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

pub(crate) fn saved_finish_relation(
    input: ValueId,
    extent: Option<FinishProjection>,
) -> Result<(), &'static str> {
    if extent
        != Some(FinishProjection {
            owner: input,
            field: 0,
        })
    {
        return Err("replay must preserve the original saved finish extent");
    }
    Ok(())
}

pub(crate) fn replay_activation_relation(
    previous_queue: ValueId,
    previous_active: ValueId,
    queue: ValueId,
    restored_parent: ValueId,
) -> Result<(), &'static str> {
    if queue != previous_queue || restored_parent != previous_active {
        return Err("replay finish must retain its accumulated queue and previous active extent");
    }
    Ok(())
}

pub(crate) fn context_finish_relation(
    context: ValueId,
    extent: Option<FinishProjection>,
    queue: Option<FinishProjection>,
    parent: Option<FinishProjection>,
) -> Result<(), &'static str> {
    for (projection, field) in [(extent, 5), (queue, 2), (parent, 3)] {
        if projection
            != Some(FinishProjection {
                owner: context,
                field,
            })
        {
            return Err("target finish must preserve its source context extent, queue and parent");
        }
    }
    Ok(())
}

/// A scoped dispatch owns the clauses of the current Handle computation.
pub(crate) fn context_clause_relation(
    computation: ValueId,
    return_clause: Option<FinishProjection>,
    operation_clause: Option<FinishProjection>,
) -> Result<(), &'static str> {
    if return_clause
        != Some(FinishProjection {
            owner: computation,
            field: 0,
        })
        || operation_clause
            != Some(FinishProjection {
                owner: computation,
                field: 1,
            })
    {
        return Err("source context must retain the current Handle return and operation clauses");
    }
    Ok(())
}

/// Tail scope entry removes one finish while sharing its completion boundary.
pub(crate) fn context_tail_relation(
    sequence: ValueId,
    saved: Option<FinishProjection>,
    parent: Option<FinishProjection>,
    extent: Option<FinishProjection>,
) -> Result<(), &'static str> {
    for (projection, field) in [(saved, 1), (parent, 2), (extent, 0)] {
        if projection
            != Some(FinishProjection {
                owner: sequence,
                field,
            })
        {
            return Err(
                "tail source context must retain queue, parent and extent from its current finish",
            );
        }
    }
    Ok(())
}

pub(crate) fn root_extent_relation(
    execution: ValueId,
    dispatch: ValueId,
) -> Result<(), &'static str> {
    if execution != dispatch {
        return Err("control run must use the same fresh root extent for execution and dispatch");
    }
    Ok(())
}

/// Which clause a source computation step applies at its current delimiter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClauseKind {
    Return,
    Operation,
}

impl ClauseKind {
    /// The scoped context field that holds this clause.
    pub(crate) fn field(self) -> u32 {
        match self {
            Self::Return => 0,
            Self::Operation => 1,
        }
    }

    /// The source computation tag this clause step interprets.
    pub(crate) fn source_tag(self) -> i32 {
        match self {
            Self::Return => 0,
            Self::Operation => 1,
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Return => "source return must apply its scoped return clause",
            Self::Operation => "source operation must apply its scoped operation clause",
        }
    }
}

/// A source `Return` applies the scoped return clause (field zero); a source
/// `Perform` applies the scoped operation clause (field one). The applied
/// callable must be that clause projection of the current scoped context, not a
/// boundary or another field of the same frame.
pub(crate) fn clause_selection_relation(
    kind: ClauseKind,
    context: ValueId,
    clause: Option<FinishProjection>,
) -> Result<(), &'static str> {
    if clause
        != Some(FinishProjection {
            owner: context,
            field: kind.field(),
        })
    {
        return Err(kind.message());
    }
    Ok(())
}

/// The first clause operand is the answer/request of the current source step.
/// The stage verifier separately connects the complete application result to
/// the next executor transition and proves that its calls execute on that path.
pub(crate) fn clause_operand_relation(
    input: ValueId,
    payload: Option<FinishProjection>,
) -> Result<(), &'static str> {
    if payload
        != Some(FinishProjection {
            owner: input,
            field: 0,
        })
    {
        return Err("source clause must consume the current computation payload");
    }
    Ok(())
}

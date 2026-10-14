//! Operand identities required by the inert Bind and raw Resume transitions.
use super::provenance::FinishProjection;
use crate::types::ValueId;

pub(crate) fn bind_relation(
    input: ValueId,
    queue: ValueId,
    inner: Option<FinishProjection>,
    callback: Option<FinishProjection>,
    tail: ValueId,
) -> Result<(), &'static str> {
    if inner
        != Some(FinishProjection {
            owner: input,
            field: 0,
        })
        || callback
            != Some(FinishProjection {
                owner: input,
                field: 1,
            })
        || tail != queue
    {
        return Err("Bind must enter its body with its own callback and current queue");
    }
    Ok(())
}

pub(crate) fn resume_answer_relation(
    input: ValueId,
    answer: Option<FinishProjection>,
) -> Result<(), &'static str> {
    if answer
        != Some(FinishProjection {
            owner: input,
            field: 0,
        })
    {
        return Err("Resume must return its current saved answer");
    }
    Ok(())
}

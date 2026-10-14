//! Ordered machine observations shared by all semantic relations and IR adapters.
use crate::cc::ReprId;
use crate::types::ValueId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Read {
        repr: ReprId,
        cell: ValueId,
        index: ValueId,
        result: ValueId,
    },
    Select {
        value: ValueId,
        tag: i32,
    },
    Store {
        repr: ReprId,
        cell: ValueId,
        index: ValueId,
        value: ValueId,
    },
    Apply {
        result: ValueId,
    },
    Advance {
        arguments: Vec<ValueId>,
    },
    Direct {
        function: psrs_hir::SymbolId,
        arguments: Vec<ValueId>,
    },
    Refine {
        result: ValueId,
        source: ValueId,
    },
    OtherCall,
    Allocate {
        result: ValueId,
    },
    Reject {
        value: ValueId,
        tags: Vec<i32>,
    },
    OtherEffect,
    Trap,
}

/// Only allocations feeding the published result or actual transition are
/// machine construction. A dead allocation is not evidence for a required node.
pub(crate) fn allocations(
    view: &impl super::execution::Operands,
    roots: &[ValueId],
    events: &[Event],
) -> Result<(), &'static str> {
    let mut pending = roots.to_vec();
    let mut reachable = std::collections::HashSet::new();
    while let Some(value) = pending.pop() {
        let value = view.origin(value);
        if reachable.insert(value) {
            pending.extend(view.inputs(value));
        }
    }
    if events.iter().any(|event| matches!(event, Event::Allocate { result } if !reachable.contains(&view.origin(*result)))) {
        Err("machine execution contains an unrelated allocation")
    } else { Ok(()) }
}

/// Trap paths retain their executed effects. Only a failed machine dispatch or
/// private live-status guard may end a step, and its effects must be a prefix of
/// a validated normal path. Thus trapping cannot hide an extra write or call.
pub(crate) fn exceptional(
    view: &impl super::owned::OwnedOperands,
    events: &[Event],
    normal: &[Vec<Event>],
    carriers: &[(ReprId, Vec<i32>)],
    status: ReprId,
) -> Result<(), &'static str> {
    let justified = events.iter().enumerate().any(|(point, event)| {
        let Event::Reject { value, tags } = event else { return false; };
        if events[point + 1..].iter().any(|event| !matches!(event, Event::Select { .. } | Event::Reject { .. })) { return false; }
        if view.integer(*value).is_some_and(|constant| tags.contains(&constant)) { return true; }
        if let Some((repr, _)) = view.tag(*value) {
            return carriers.iter().any(|(candidate, domain)| *candidate == repr && tags == domain);
        }
        tags == &[0] && events.iter().any(|event| matches!(event, Event::Read { repr, index, result, .. } if *repr == status && view.integer(*index) == Some(0) && view.origin(*result) == view.origin(*value)))
    });
    let observable = |event: &Event| !matches!(event, Event::Select { .. } | Event::Reject { .. });
    let prefix = events
        .iter()
        .filter(|event| observable(event))
        .collect::<Vec<_>>();
    if justified
        && normal.iter().any(|path| {
            let effects = path
                .iter()
                .filter(|event| observable(event))
                .collect::<Vec<_>>();
            effects.starts_with(&prefix)
        })
    {
        Ok(())
    } else {
        Err("exceptional machine path has unaccounted effects or no failed guard")
    }
}

pub(crate) fn carriers(
    table: &crate::cc::RepresentationTable,
) -> Result<Vec<(ReprId, Vec<i32>)>, &'static str> {
    let checkpoint = table
        .control_checkpoint_schema()
        .ok_or("execution has no checkpoint schema")?;
    let replay = table
        .control_replay_schema()
        .ok_or("execution has no replay schema")?;
    let owner = super::owned::OwnedSchema::checked(table)?;
    [
        checkpoint.computation,
        checkpoint.context,
        replay.sequence,
        replay.boundary,
        owner.owner,
    ]
    .into_iter()
    .map(|repr| {
        let Some(crate::cc::Representation::Variant { cases }) = table.representation(repr) else {
            return Err("execution carrier is not a variant");
        };
        Ok((repr, cases.iter().map(|case| case.tag as i32).collect()))
    })
    .collect()
}

/// Operand identity is path-sensitive. Keep instruction result IDs (and hence
/// their immutable definitions), but resolve every observed input through that
/// path's aliases before comparing prefixes across joins.
pub(crate) fn canonicalize(events: &mut [Event], origin: impl Fn(ValueId) -> ValueId) {
    for event in events {
        match event {
            Event::Read { cell, index, .. } => {
                *cell = origin(*cell);
                *index = origin(*index);
            }
            Event::Select { value, .. } | Event::Reject { value, .. } => *value = origin(*value),
            Event::Store {
                cell, index, value, ..
            } => {
                *cell = origin(*cell);
                *index = origin(*index);
                *value = origin(*value);
            }
            Event::Advance { arguments } | Event::Direct { arguments, .. } => {
                for value in arguments {
                    *value = origin(*value);
                }
            }
            Event::Refine { source, .. } => *source = origin(*source),
            Event::Apply { .. }
            | Event::Allocate { .. }
            | Event::OtherCall
            | Event::OtherEffect
            | Event::Trap => {}
        }
    }
}

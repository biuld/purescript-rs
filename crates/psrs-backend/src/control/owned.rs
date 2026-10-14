//! One-shot execution is an ordered transfer of private owner storage.
use super::execution::{Operands, Schema};
use crate::cc::{ReprId, Representation, RepresentationTable, ValueShape};
use crate::types::ValueId;

pub(crate) struct OwnedSchema {
    pub owner: ReprId,
    pub callback: ReprId,
    pub status: ReprId,
}

impl OwnedSchema {
    pub fn checked(table: &RepresentationTable) -> Result<Self, &'static str> {
        let schema = table
            .control_schema()
            .ok_or("owned execution has no checked machine placement")?;
        if !matches!(table.representation(schema.callback), Some(Representation::Array { element }) if *element == crate::cc::payload::erased_shape())
            || !matches!(
                table.representation(schema.status),
                Some(Representation::Array {
                    element: ValueShape::Integer
                })
            )
        {
            return Err("owned execution has invalid private storage");
        }
        Ok(Self {
            owner: schema.owner,
            callback: schema.callback,
            status: schema.status,
        })
    }
}

pub(crate) trait OwnedOperands: Operands {
    fn read(&self, value: ValueId, repr: ReprId) -> Option<(ValueId, i32)>;
    fn integer(&self, value: ValueId) -> Option<i32>;
    fn initial(&self, cell: ValueId, repr: ReprId) -> Option<ValueId>;
}

use super::events::Event;

pub(crate) fn verify(
    schema: &Schema,
    owned: &OwnedSchema,
    view: &impl OwnedOperands,
    current: &[ValueId],
    next: &[ValueId],
    events: &[Event],
) -> Result<(), &'static str> {
    if current.len() != 6 || next.len() != 6 {
        return Err("owned execution has invalid transition arguments");
    }
    for index in [0, 1, 3, 4, 5] {
        if view.origin(current[index]) != view.origin(next[index]) {
            return Err("owned execution must preserve state, handler, queue, extent and context");
        }
    }
    let (callee, arguments) = view
        .application(next[2])
        .filter(|(_, arguments)| arguments.len() == 1)
        .ok_or("owned execution must consume its callback result")?;
    if view.projection(arguments[0], schema.computation, Some(5))
        != Some((view.origin(current[2]), 1))
    {
        return Err("owned execution must pass its current answer");
    }
    let (callback, 0) = view
        .read(callee, owned.callback)
        .ok_or("owned execution must load its own callback slot")?
    else {
        return Err("owned execution must load its own callback slot");
    };
    let (owner, 0) = view
        .projection(callback, owned.owner, Some(0))
        .ok_or("owned execution must load its own callback slot")?
    else {
        return Err("owned execution must load its own callback slot");
    };
    if view.projection(owner, schema.computation, Some(5)) != Some((view.origin(current[2]), 0)) {
        return Err("owned execution must consume its current owner");
    }
    let is_status = |value| view.projection(value, owned.owner, Some(0)) == Some((owner, 1));
    let mut pending = None;
    let mut running = None;
    let mut loaded = None;
    let mut cleared = None;
    let mut called = None;
    let mut consumed = None;
    let mut advanced = None;
    for (point, event) in events.iter().enumerate() {
        match event {
            Event::Refine { .. } => {}
            Event::Read {
                repr,
                cell,
                index,
                result,
            } => {
                if view.integer(*index) != Some(0) {
                    return Err("owned execution must access slot zero");
                }
                if *repr == owned.callback
                    && view.origin(*cell) == callback
                    && view.origin(*result) == view.origin(callee)
                {
                    if loaded.replace(point).is_some() {
                        return Err("owned execution must load its callback exactly once");
                    }
                } else if *repr != owned.status || !is_status(*cell) {
                    return Err("owned execution must read its own private storage");
                }
            }
            Event::Select { value, tag } => {
                if let Some((cell, 0)) = view.read(*value, owned.status)
                    && (!is_status(cell) || *tag != 0 || pending.replace(point).is_some())
                {
                    return Err("owned execution must establish Pending exactly once");
                }
            }
            Event::Store {
                repr,
                cell,
                index,
                value,
            } => {
                if view.integer(*index) != Some(0) {
                    return Err("owned execution must access slot zero");
                }
                if *repr == owned.status && is_status(*cell) {
                    match view.integer(*value) {
                        Some(1) if running.replace(point).is_none() => {}
                        Some(2) if consumed.replace(point).is_none() => {}
                        _ => {
                            return Err(
                                "owned execution must transfer Pending to Running then Consumed",
                            );
                        }
                    }
                } else if *repr == owned.callback
                    && view.origin(*cell) == callback
                    && is_status(*value)
                    && cleared.replace(point).is_none()
                {
                } else {
                    return Err("owned execution must clear its callback with its own status cell");
                }
            }
            Event::Apply { result }
                if view.origin(*result) == view.origin(next[2])
                    && called.replace(point).is_none() => {}
            Event::Advance { arguments }
                if arguments.len() == next.len()
                    && arguments
                        .iter()
                        .zip(next)
                        .all(|(left, right)| view.origin(*left) == view.origin(*right))
                    && advanced.replace(point).is_none() => {}
            _ => return Err("owned execution contains an additional observable operation"),
        }
    }
    let Some(points) = pending
        .zip(running)
        .zip(loaded)
        .zip(cleared)
        .zip(called)
        .zip(consumed)
        .zip(advanced)
    else {
        return Err("owned execution is missing its required transfer steps");
    };
    let ((((((pending, running), loaded), cleared), called), consumed), advanced) = points;
    if !(pending < running
        && running < loaded
        && loaded < cleared
        && cleared < called
        && called < consumed
        && consumed < advanced)
    {
        return Err("owned execution must guard, acquire, load, clear, call and consume in order");
    }
    Ok(())
}

/// A constructor proves only initial contents. Replay ordered private stores
/// and reads before publication; a later store must not inherit stale evidence.
pub(crate) enum Expected {
    Value(ValueId),
    Integer(i32),
}

pub(crate) fn publication(
    view: &impl OwnedOperands,
    roots: &[ValueId],
    cells: &[(ValueId, ReprId, Expected)],
    events: &[Event],
    total: &std::collections::HashSet<psrs_hir::SymbolId>,
) -> Result<(), &'static str> {
    let mut contents = std::collections::HashMap::new();
    let mut observed = std::collections::HashMap::new();
    for (cell, repr, _) in cells {
        let value = view
            .initial(*cell, *repr)
            .ok_or("control factory must allocate fresh one-slot private cells")?;
        contents.insert(view.origin(*cell), view.origin(value));
    }
    let resolve = |mut value: ValueId, observed: &std::collections::HashMap<ValueId, ValueId>| {
        value = view.origin(value);
        for _ in 0..observed.len() {
            if let Some(next) = observed.get(&value) {
                value = *next
            } else {
                break;
            }
        }
        value
    };
    for event in events {
        match event {
            Event::Refine { .. } => {}
            Event::Read {
                cell,
                index,
                result,
                ..
            } => {
                if let Some(value) = contents.get(&view.origin(*cell)) {
                    if view.integer(*index) != Some(0) {
                        return Err("control factory must access private slot zero");
                    }
                    observed.insert(view.origin(*result), *value);
                } else {
                    return Err("control factory must read its own private storage");
                }
            }
            Event::Store {
                cell, index, value, ..
            } => {
                if let Some(content) = contents.get_mut(&view.origin(*cell)) {
                    if view.integer(*index) != Some(0) {
                        return Err("control factory must access private slot zero");
                    }
                    *content = resolve(*value, &observed);
                } else {
                    return Err("control factory must write its own private storage");
                }
            }
            Event::Direct { function, .. } if total.contains(function) => {}
            Event::Direct { .. }
            | Event::Apply { .. }
            | Event::Advance { .. }
            | Event::OtherCall => {
                return Err("control factory must not invoke callbacks before publication");
            }
            Event::Allocate { .. } | Event::Select { .. } | Event::Reject { .. } => {}
            _ => return Err("control factory contains an additional observable operation"),
        }
    }
    super::events::allocations(view, roots, events)?;
    for (cell, _, expected) in cells {
        let actual = contents[&view.origin(*cell)];
        let matches = match expected {
            Expected::Value(value) => actual == resolve(*value, &observed),
            Expected::Integer(value) => view.integer(actual) == Some(*value),
        };
        if !matches {
            return Err("control factory must publish its required private cell contents");
        }
    }
    Ok(())
}

pub(crate) fn cancel(
    owned: &OwnedSchema,
    view: &impl OwnedOperands,
    owner: ValueId,
    result: ValueId,
    events: &[Event],
) -> Result<(), &'static str> {
    if view.origin(owner) != view.origin(result) {
        return Err("cancellation must retain its current owner");
    }
    let field = |value, index| {
        view.projection(value, owned.owner, Some(0)) == Some((view.origin(owner), index))
    };
    let mut pending = None;
    let mut cancelled = None;
    let mut cleared = None;
    for (point, event) in events.iter().enumerate() {
        match event {
            Event::Refine { .. } => {}
            Event::Read {
                repr, cell, index, ..
            } if *repr == owned.status && field(*cell, 1) && view.integer(*index) == Some(0) => {}
            Event::Select { value, tag } => {
                if let Some((cell, 0)) = view.read(*value, owned.status)
                    && (!field(cell, 1) || *tag != 0 || pending.replace(point).is_some())
                {
                    return Err("cancellation must establish Pending exactly once");
                }
            }
            Event::Store {
                repr,
                cell,
                index,
                value,
            } if view.integer(*index) == Some(0) => {
                let cancel = *repr == owned.status
                    && field(*cell, 1)
                    && view.integer(*value) == Some(3)
                    && cancelled.replace(point).is_none();
                let clear = *repr == owned.callback
                    && field(*cell, 0)
                    && field(*value, 1)
                    && cleared.replace(point).is_none();
                if !(cancel || clear) {
                    return Err("cancellation must cancel and clear its own private storage");
                }
            }
            _ => return Err("cancellation contains an additional observable operation"),
        }
    }
    if !matches!((pending, cancelled, cleared), (Some(pending), Some(cancelled), Some(cleared)) if pending < cancelled && cancelled < cleared)
    {
        return Err(
            "cancellation must guard Pending, mark Cancelled and clear the callback in order",
        );
    }
    Ok(())
}

use super::{Facts, Operands, Schema, field, find_field};
use crate::types::ValueId;

/// Every dynamic application on a root/queue path is part of its validated
/// transition. Close is the sole static effect of an ordinary Finish step.
pub(crate) fn verify(
    schema: &Schema,
    view: &impl Operands,
    facts: &Facts,
    current: &[ValueId],
    next: Option<&[ValueId]>,
    events: &[crate::control::events::Event],
    total: &std::collections::HashSet<psrs_hir::SymbolId>,
) -> Result<(), &'static str> {
    use crate::control::events::Event;
    if current.len() != 6 {
        return Err("execution has invalid arguments");
    }
    let selected = |repr, value| facts.get(&(repr, view.origin(value))).copied();
    let tag = selected(schema.computation, current[2]);
    if !matches!(tag, Some(0..=4)) {
        return Ok(());
    }
    let context = selected(schema.context, current[5]);
    let queue = selected(schema.queue, current[3]);
    let count = if !matches!(tag, Some(0 | 1)) {
        0
    } else if tag == Some(1) {
        if context == Some(0) { 3 } else { 2 }
    } else if queue == Some(1)
        || (queue == Some(0) && context == Some(1))
        || (queue == Some(2)
            && find_field(view, current[3], schema.queue, 2, 0, facts)
                .is_ok_and(|left| selected(schema.queue, left) == Some(1)))
    {
        1
    } else {
        0
    };
    let mut expected = std::collections::HashSet::new();
    if count > 0 {
        let next = next.ok_or("execution is missing its required application")?;
        let mut result = if tag == Some(1) && context == Some(0) {
            view.projection(next[0], schema.pair, None)
                .ok_or("root operation has no handler result")?
                .0
        } else {
            next[2]
        };
        for _ in 0..count {
            expected.insert(view.origin(result));
            result = view
                .application(result)
                .ok_or("execution is missing its required application")?
                .0;
        }
    }
    let finish = tag == Some(0) && queue == Some(3);
    let mut closed = false;
    let mut advanced = false;
    let mut checked = false;
    let mut cloned = false;
    for event in events {
        match event {
            Event::Apply { result } if expected.remove(&view.origin(*result)) => {}
            Event::Advance { arguments }
                if !advanced
                    && next.is_some_and(|next| {
                        arguments.len() == next.len()
                            && arguments
                                .iter()
                                .zip(next)
                                .all(|(left, right)| view.origin(*left) == view.origin(*right))
                    }) =>
            {
                advanced = true;
            }
            Event::Direct {
                function,
                arguments,
            } if finish
                && !closed
                && *function == schema.close
                && arguments.len() == 1
                && field(view, arguments[0], schema.queue, 3, 0, current[3]) =>
            {
                closed = true;
            }
            Event::Direct {
                function,
                arguments,
            } if tag == Some(3)
                && !checked
                && *function == schema.check
                && arguments.len() == 1
                && field(view, arguments[0], schema.computation, 3, 2, current[2]) =>
            {
                checked = true;
            }
            Event::Direct {
                function,
                arguments,
            } if tag == Some(3)
                && !cloned
                && *function == schema.clone
                && arguments.len() == 5
                && field(view, arguments[0], schema.computation, 3, 1, current[2]) =>
            {
                cloned = true;
            }
            Event::Direct { function, .. } if total.contains(function) => {}
            Event::Apply { .. }
            | Event::Advance { .. }
            | Event::Direct { .. }
            | Event::OtherCall
            | Event::OtherEffect
            | Event::Trap
            | Event::Read { .. }
            | Event::Store { .. } => {
                return Err("root or queue execution contains an additional observable operation");
            }
            _ => {}
        }
    }
    if !expected.is_empty()
        || advanced != next.is_some()
        || closed != finish
        || checked != (tag == Some(3))
        || cloned != (tag == Some(3))
    {
        return Err("root or queue execution is missing its required effects");
    }
    Ok(())
}

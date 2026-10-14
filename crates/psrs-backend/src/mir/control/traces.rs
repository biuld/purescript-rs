//! Normalize executed effects without rebuilding scalar or reference identities.
use crate::cc::{ReprId, Representation};
use crate::control::events::Event;
use crate::control::execution::Operands;
use crate::mir::branch_facts::Exit;
use crate::mir::layout::PlannedLayout;
use crate::mir::{Function, Instruction as I, Terminator as T};
use std::collections::HashMap;

pub(super) fn events(
    function: &Function,
    layout: &PlannedLayout,
    exit: &Exit,
) -> Result<Vec<Event>, &'static str> {
    let arrays = layout
        .representation_table()
        .representations
        .iter()
        .enumerate()
        .filter_map(|(index, repr)| {
            matches!(repr, Representation::Array { .. }).then_some(ReprId(index as u32))
        })
        .filter_map(|repr| layout.repr_index(repr).ok().map(|index| (index, repr)))
        .collect::<HashMap<_, _>>();
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let definitions = crate::mir::value_identity::definitions(function);
    let raw = layout
        .representation_table()
        .control_resumption
        .as_ref()
        .ok_or("execution has no raw contract")?
        .function();
    let view = super::execution::View {
        definitions: &definitions,
        layout,
        aliases: &exit.aliases,
        raw,
    };
    let mut events = Vec::new();
    for (id, index) in &exit.points {
        let block = blocks.get(id).ok_or("execution trace has no block")?;
        let Some(instruction) = block.instructions.get(*index) else {
            for (value, tag, point) in &exit.selections {
                if point == &(*id, *index) {
                    events.push(Event::Select {
                        value: *value,
                        tag: *tag,
                    });
                }
            }
            for (value, tags, point) in &exit.rejections {
                if point == &(*id, *index) {
                    events.push(Event::Reject {
                        value: *value,
                        tags: tags.clone(),
                    });
                }
            }
            if *id == exit.block {
                match block
                    .terminator
                    .as_ref()
                    .ok_or("execution trace has no exit")?
                {
                    T::Jump { arguments, .. } | T::ReturnCall { arguments, .. } => {
                        events.push(Event::Advance {
                            arguments: arguments.clone(),
                        })
                    }
                    _ => {}
                }
            }
            continue;
        };
        let event = match instruction {
            I::RefCast {
                value, reference, ..
            } if !exit.trapped
                && !super::refinements::checked(
                    &view, function, layout, exit, *value, *reference,
                ) =>
            {
                Some(Event::Trap)
            }
            I::RefCast {
                destination, value, ..
            } => Some(Event::Refine {
                result: *destination,
                source: crate::mir::value_identity::path_origin(
                    *value,
                    &definitions,
                    &exit.aliases,
                ),
            }),
            I::Unreachable { .. }
                if exit.trapped && *id == exit.block && *index + 1 == block.instructions.len() =>
            {
                None
            }
            I::ArrayGet {
                type_index,
                value,
                index,
                destination,
                ..
            } => Some(Event::Read {
                repr: *arrays
                    .get(type_index)
                    .ok_or("execution reads an unplanned array")?,
                cell: *value,
                index: *index,
                result: *destination,
            }),
            I::ArraySet {
                type_index,
                value,
                index,
                new_value,
                ..
            } => Some(Event::Store {
                repr: *arrays
                    .get(type_index)
                    .ok_or("execution writes an unplanned array")?,
                cell: *value,
                index: *index,
                value: *new_value,
            }),
            I::StructNew { destination, .. }
            | I::ClosureNew { destination, .. }
            | I::ArrayNew { destination, .. }
            | I::ArrayNewSized { destination, .. }
            | I::ArrayNewFilled { destination, .. }
            | I::ArrayNewData { destination, .. } => Some(Event::Allocate {
                result: *destination,
            }),
            I::TrapIf { .. }
            | I::Unreachable { .. }
            | I::Primitive {
                op: crate::mir::NumericOp::I32DivS | crate::mir::NumericOp::I32RemS,
                ..
            } => Some(Event::Trap),
            I::ClosureCall { destination, .. } => Some(Event::Apply {
                result: *destination,
            }),
            I::Call {
                function: target,
                arguments,
                ..
            } if *target == function.symbol => Some(Event::Advance {
                arguments: arguments.clone(),
            }),
            I::Call {
                function,
                arguments,
                ..
            }
            | I::CallVoid {
                function,
                arguments,
                ..
            } => Some(Event::Direct {
                function: *function,
                arguments: arguments.clone(),
            }),
            I::StringToBytes { .. } | I::ArrayClone { .. } => Some(Event::OtherEffect),
            instruction if crate::mir::effects::may_invoke(instruction) => Some(Event::OtherCall),
            instruction if crate::mir::effects::classify(instruction).writes_memory => {
                Some(Event::OtherEffect)
            }
            instruction
                if crate::mir::effects::classify(instruction).reads_memory
                    && !matches!(
                        instruction,
                        I::StructGet { .. } | I::ClosureGetCapture { .. }
                    ) =>
            {
                Some(Event::OtherEffect)
            }
            _ => None,
        };
        if let Some(event) = event {
            events.push(event)
        }
    }
    crate::control::events::canonicalize(&mut events, |value| view.origin(value));
    Ok(events)
}

/// Loop initialization is part of helper execution, before its first dispatch.
pub(super) fn entry(function: &Function, layout: &PlannedLayout) -> Result<(), &'static str> {
    let Some(header) = crate::mir::value_identity::loop_header(function) else {
        return Ok(());
    };
    let definitions = crate::mir::value_identity::definitions(function);
    let raw = layout
        .representation_table()
        .control_resumption
        .as_ref()
        .ok_or("entry has no raw contract")?
        .function();
    for path in crate::mir::branch_facts::paths(function, function.entry, Some(header))? {
        if path.trapped {
            return Err("machine entry cannot trap before dispatch");
        }
        let view = super::execution::View {
            definitions: &definitions,
            layout,
            aliases: &path.aliases,
            raw,
        };
        let observed = events(function, layout, &path)?;
        let [Event::Advance { arguments }] = observed.as_slice() else {
            return Err("machine entry contains an additional observable operation");
        };
        if arguments.len() != function.parameters.len()
            || !arguments
                .iter()
                .zip(&function.parameters)
                .all(|(a, b)| view.origin(*a) == view.origin(*b))
        {
            return Err("machine entry must initialize its own parameters");
        }
    }
    Ok(())
}

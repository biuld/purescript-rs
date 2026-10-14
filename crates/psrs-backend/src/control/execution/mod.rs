//! The root fold and persistent queue have one operational contract across IRs.
use crate::cc::{ReprId, Representation, RepresentationTable};
use crate::types::ValueId;
use std::collections::HashMap;

pub(crate) type Facts = HashMap<(ReprId, ValueId), i32>;

pub(crate) struct Schema {
    pub computation: ReprId,
    pub queue: ReprId,
    pub context: ReprId,
    pub pair: ReprId,
    pub close: psrs_hir::SymbolId,
    pub check: psrs_hir::SymbolId,
    pub clone: psrs_hir::SymbolId,
}

impl Schema {
    pub fn checked(table: &RepresentationTable) -> Result<Self, &'static str> {
        let source = table
            .control_checkpoint_schema()
            .ok_or("execution has no checked source schema")?;
        let pair = table
            .control_result_pair
            .ok_or("execution has no planned result pair")?;
        if !matches!(table.representation(pair), Some(Representation::Product { fields })
            if fields == &[crate::cc::payload::erased_shape(); 2])
        {
            return Err("execution result pair has an invalid planned representation");
        }
        Ok(Self {
            computation: source.computation,
            queue: table
                .control_sequence()
                .ok_or("execution has no checked queue schema")?,
            context: source.context,
            pair,
            close: table
                .control_helpers
                .iter()
                .find(|helper| helper.role() == crate::cc::ControlHelperRole::Close)
                .ok_or("execution has no Close contract")?
                .function(),
            check: table
                .control_helpers
                .iter()
                .find(|helper| helper.role() == crate::cc::ControlHelperRole::Check)
                .ok_or("execution has no Check contract")?
                .function(),
            clone: table
                .control_helpers
                .iter()
                .find(|helper| helper.role() == crate::cc::ControlHelperRole::CloneSequence)
                .ok_or("execution has no CloneSequence contract")?
                .function(),
        })
    }
}

/// Each adapter discharges its IR's explicit operations into operand evidence.
/// Generic SSA/type verification separately checks availability and legality.
pub(crate) trait Operands {
    fn inputs(&self, value: ValueId) -> Vec<ValueId>;
    fn tag(&self, value: ValueId) -> Option<(ReprId, ValueId)>;
    fn origin(&self, value: ValueId) -> ValueId;
    fn projection(&self, value: ValueId, repr: ReprId, case: Option<u32>)
    -> Option<(ValueId, u32)>;
    fn node(&self, value: ValueId, repr: ReprId, case: Option<u32>) -> Option<Vec<ValueId>>;
    fn application(&self, value: ValueId) -> Option<(ValueId, Vec<ValueId>)>;
    fn raw_resumption(&self, value: ValueId) -> bool;
}

fn field(
    view: &impl Operands,
    value: ValueId,
    repr: ReprId,
    tag: u32,
    index: u32,
    owner: ValueId,
) -> bool {
    view.projection(value, repr, Some(tag)) == Some((view.origin(owner), index))
}

fn apply(
    view: &impl Operands,
    mut result: ValueId,
    callee: ValueId,
    arguments: &[ValueId],
) -> bool {
    for argument in arguments.iter().rev() {
        let Some((function, inputs)) = view.application(result) else {
            return false;
        };
        if inputs.len() != 1 || view.origin(inputs[0]) != view.origin(*argument) {
            return false;
        }
        result = function;
    }
    view.origin(result) == view.origin(callee)
}

pub(crate) fn transition(
    schema: &Schema,
    view: &impl Operands,
    facts: &Facts,
    current: &[ValueId],
    next: &[ValueId],
) -> Result<(), &'static str> {
    if current.len() != 6 || next.len() != 6 {
        return Err("execution has an invalid executor transition");
    }
    let selected = |repr, value| facts.get(&(repr, view.origin(value))).copied();
    match selected(schema.computation, current[2]) {
        Some(1) if selected(schema.context, current[5]) == Some(0) => {
            root_request(schema, view, current, next)
        }
        Some(0) => returned(schema, view, facts, current, next),
        Some(1..=5) => Ok(()), // Other owning relations check these steps.
        _ => Err("execution exit has no current computation selection"),
    }
}

fn root_request(
    schema: &Schema,
    view: &impl Operands,
    current: &[ValueId],
    next: &[ValueId],
) -> Result<(), &'static str> {
    for index in [1, 4, 5] {
        if view.origin(next[index]) != view.origin(current[index]) {
            return Err("root operation must preserve handler, extent and context");
        }
    }
    if view.node(next[3], schema.queue, Some(0)).as_deref() != Some(&[]) {
        return Err("root operation must clear its captured source queue");
    }
    let pair = view
        .projection(next[0], schema.pair, None)
        .filter(|(_, field)| *field == 0)
        .ok_or("root operation must consume the same handler result pair")?
        .0;
    if view.projection(next[2], schema.pair, None) != Some((pair, 1)) {
        return Err("root operation must consume the same handler result pair");
    }
    let Some((after_request, resume)) = view.application(pair) else {
        return Err("root operation must apply its complete handler");
    };
    let Some((after_state, request)) = view.application(after_request) else {
        return Err("root operation must apply its complete handler");
    };
    if resume.len() != 1
        || request.len() != 1
        || !view.raw_resumption(resume[0])
        || !field(view, request[0], schema.computation, 1, 0, current[2])
        || !apply(view, after_state, current[1], &[current[0]])
    {
        return Err(
            "root operation must apply its current handler to state, request and raw resumption",
        );
    }
    Ok(())
}

fn returned(
    schema: &Schema,
    view: &impl Operands,
    facts: &Facts,
    current: &[ValueId],
    next: &[ValueId],
) -> Result<(), &'static str> {
    let selected = |value| facts.get(&(schema.queue, view.origin(value))).copied();
    if selected(current[3]) == Some(0) {
        return if facts.get(&(schema.context, view.origin(current[5]))) == Some(&1) {
            Ok(())
        } else {
            Err("root return must publish its state and current answer")
        };
    }
    for index in [0, 1, 5] {
        if view.origin(next[index]) != view.origin(current[index]) {
            return Err("queue execution must preserve state, handler and context");
        }
    }
    let queue = current[3];
    let finish = selected(queue) == Some(3);
    if !finish && view.origin(next[4]) != view.origin(current[4]) {
        return Err("queue execution must preserve its current extent");
    }
    match selected(queue) {
        Some(1) => callback(schema, view, current[2], queue, next[2], next[3]),
        Some(2) => {
            let left = find_field(view, queue, schema.queue, 2, 0, facts)?;
            append(schema, view, facts, current, next, left)
        }
        Some(3)
            if view.origin(next[2]) == view.origin(current[2])
                && field(view, next[3], schema.queue, 3, 1, queue)
                && field(view, next[4], schema.queue, 3, 2, queue) =>
        {
            Ok(())
        }
        _ => Err("queue execution must consume its selected current frame"),
    }
}

/// The selected Append left operand is retained in facts by its projection ID.
/// Do not invent a value when the producer's projection evidence is absent.
fn find_field(
    view: &impl Operands,
    owner: ValueId,
    repr: ReprId,
    tag: u32,
    field_index: u32,
    facts: &Facts,
) -> Result<ValueId, &'static str> {
    facts
        .keys()
        .filter(|(candidate, _)| *candidate == repr)
        .map(|(_, value)| *value)
        .find(|value| field(view, *value, repr, tag, field_index, owner))
        .ok_or("Append must dispatch on its current left queue")
}

fn callback(
    schema: &Schema,
    view: &impl Operands,
    input: ValueId,
    queue: ValueId,
    computation: ValueId,
    rest: ValueId,
) -> Result<(), &'static str> {
    let Some((callee, answer)) = view.application(computation) else {
        return Err("queue callback result must enter the next computation");
    };
    if answer.len() != 1
        || !field(view, callee, schema.queue, 1, 0, queue)
        || !field(view, answer[0], schema.computation, 0, 0, input)
        || !field(view, rest, schema.queue, 1, 1, queue)
    {
        return Err("queue callback must consume its current answer, callback and tail");
    }
    Ok(())
}

fn append(
    schema: &Schema,
    view: &impl Operands,
    facts: &Facts,
    current: &[ValueId],
    next: &[ValueId],
    left: ValueId,
) -> Result<(), &'static str> {
    let right = |value| field(view, value, schema.queue, 2, 1, current[3]);
    let same = view.origin(next[2]) == view.origin(current[2]);
    match facts.get(&(schema.queue, view.origin(left))).copied() {
        Some(0) if same && right(next[3]) => Ok(()),
        Some(1) => {
            let node = view
                .node(next[3], schema.queue, Some(2))
                .filter(|fields| fields.len() == 2)
                .ok_or("Append callback must retain its current right queue")?;
            if !right(node[1]) {
                return Err("Append callback must retain its current right queue");
            }
            callback(schema, view, current[2], left, next[2], node[0])
        }
        Some(2) if same => {
            let node = view
                .node(next[3], schema.queue, Some(2))
                .filter(|fields| fields.len() == 2)
                .ok_or("Append reassociation must preserve left-to-right queue order")?;
            let tail = view
                .node(node[1], schema.queue, Some(2))
                .filter(|fields| fields.len() == 2)
                .ok_or("Append reassociation must preserve left-to-right queue order")?;
            if field(view, node[0], schema.queue, 2, 0, left)
                && field(view, tail[0], schema.queue, 2, 1, left)
                && right(tail[1])
            {
                Ok(())
            } else {
                Err("Append reassociation must preserve left-to-right queue order")
            }
        }
        Some(3) if same => {
            let node = view
                .node(next[3], schema.queue, Some(3))
                .filter(|fields| fields.len() == 3)
                .ok_or("Append must retain the complete left finish frame")?;
            let tail = view
                .node(node[1], schema.queue, Some(2))
                .filter(|fields| fields.len() == 2)
                .ok_or("Append must retain the complete left finish frame")?;
            if field(view, node[0], schema.queue, 3, 0, left)
                && field(view, node[2], schema.queue, 3, 2, left)
                && field(view, tail[0], schema.queue, 3, 1, left)
                && right(tail[1])
            {
                Ok(())
            } else {
                Err("Append must retain the complete left finish frame")
            }
        }
        _ => Err("Append must consume its selected current left frame"),
    }
}

pub(crate) fn publish(
    schema: &Schema,
    view: &impl Operands,
    facts: &Facts,
    current: &[ValueId],
    result: ValueId,
) -> Result<(), &'static str> {
    if current.len() != 6
        || facts.get(&(schema.computation, view.origin(current[2]))) != Some(&0)
        || facts.get(&(schema.queue, view.origin(current[3]))) != Some(&0)
        || facts.get(&(schema.context, view.origin(current[5]))) != Some(&0)
    {
        return Err("execution result must be its terminal root return");
    }
    let fields = view
        .node(result, schema.pair, None)
        .filter(|fields| fields.len() == 2)
        .ok_or("root return must publish its state and current answer")?;
    if view.origin(fields[0]) != view.origin(current[0])
        || !field(view, fields[1], schema.computation, 0, 0, current[2])
    {
        return Err("root return must publish its state and current answer");
    }
    Ok(())
}

/// Coverage is measured only on validated actual exits. Trapping defaults
/// remain legal, but cannot stand in for a required root or queue action.
#[derive(Default)]
pub(crate) struct Coverage(std::collections::HashSet<(i32, i32)>);

impl Coverage {
    pub fn observe(
        &mut self,
        schema: &Schema,
        view: &impl Operands,
        facts: &Facts,
        current: &[ValueId],
    ) {
        if current.len() != 6 {
            return;
        }
        let selected = |repr, value| facts.get(&(repr, view.origin(value))).copied();
        match selected(schema.computation, current[2]) {
            Some(1) if selected(schema.context, current[5]) == Some(0) => {
                self.0.insert((1, 0));
            }
            Some(0) => match selected(schema.queue, current[3]) {
                Some(0) if selected(schema.context, current[5]) == Some(0) => {
                    self.0.insert((0, 0));
                }
                Some(1 | 3) => {
                    self.0
                        .insert((0, selected(schema.queue, current[3]).unwrap()));
                }
                Some(2) => {
                    if let Ok(left) = find_field(view, current[3], schema.queue, 2, 0, facts)
                        && let Some(tag @ 0..=3) = selected(schema.queue, left)
                    {
                        self.0.insert((2, tag));
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn finish(&self) -> Result<(), &'static str> {
        if [
            (1, 0),
            (0, 0),
            (0, 1),
            (0, 3),
            (2, 0),
            (2, 1),
            (2, 2),
            (2, 3),
        ]
        .iter()
        .all(|action| self.0.contains(action))
        {
            Ok(())
        } else {
            Err("execution must retain every root and queue action")
        }
    }
}

mod observables;
pub(crate) use observables::verify as effects;

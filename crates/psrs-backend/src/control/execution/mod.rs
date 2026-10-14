//! The root fold and persistent queue have one operational contract across IRs.
use crate::cc::{ReprId, Representation, RepresentationTable};
use crate::control::schema::{ComputationCase, ContextCase, SequenceCase};
use crate::control::schema::{computation as c, sequence as q};
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
        let placed = table
            .control_schema()
            .ok_or("execution has no checked machine placement")?;
        let pair = placed.result_pair;
        if !matches!(table.representation(pair), Some(Representation::Product { fields })
            if fields == &[crate::cc::payload::erased_shape(); 2])
        {
            return Err("execution result pair has an invalid planned representation");
        }
        let helper = |role| {
            table
                .control_helper(role)
                .ok_or("execution has no checked helper contract")
                .map(crate::cc::ControlHelperContract::function)
        };
        Ok(Self {
            computation: placed.computation,
            queue: placed.sequence,
            context: placed.context,
            pair,
            close: helper(crate::cc::ControlHelperRole::Close)?,
            check: helper(crate::cc::ControlHelperRole::Check)?,
            clone: helper(crate::cc::ControlHelperRole::CloneSequence)?,
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
        Some(tag)
            if tag == ComputationCase::Perform.tag() as i32
                && selected(schema.context, current[5]) == Some(ContextCase::Root.tag() as i32) =>
        {
            root_request(schema, view, current, next)
        }
        Some(tag) if tag == ComputationCase::Return.tag() as i32 => {
            returned(schema, view, facts, current, next)
        }
        Some(tag)
            if (ComputationCase::Perform.tag() as i32
                ..=ComputationCase::OwnedResume.tag() as i32)
                .contains(&tag) =>
        {
            Ok(()) // Other owning relations check these steps.
        }
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
    if view
        .node(next[3], schema.queue, Some(SequenceCase::Empty.tag()))
        .as_deref()
        != Some(&[])
    {
        return Err("root operation must clear its captured source queue");
    }
    let pair = view
        .projection(next[0], schema.pair, None)
        .filter(|(_, field)| *field == crate::control::schema::result_pair::STATE)
        .ok_or("root operation must consume the same handler result pair")?
        .0;
    if view.projection(next[2], schema.pair, None)
        != Some((pair, crate::control::schema::result_pair::VALUE))
    {
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
        || !field(
            view,
            request[0],
            schema.computation,
            ComputationCase::Perform.tag(),
            c::PAYLOAD,
            current[2],
        )
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
    if selected(current[3]) == Some(SequenceCase::Empty.tag() as i32) {
        return if facts.get(&(schema.context, view.origin(current[5])))
            == Some(&(ContextCase::Scoped.tag() as i32))
        {
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
    let finish = selected(queue) == Some(SequenceCase::Finish.tag() as i32);
    if !finish && view.origin(next[4]) != view.origin(current[4]) {
        return Err("queue execution must preserve its current extent");
    }
    match selected(queue) {
        Some(tag) if tag == SequenceCase::Cons.tag() as i32 => {
            callback(schema, view, current[2], queue, next[2], next[3])
        }
        Some(tag) if tag == SequenceCase::Append.tag() as i32 => {
            let left = find_field(
                view,
                queue,
                schema.queue,
                SequenceCase::Append.tag(),
                q::APPEND_LEFT,
                facts,
            )?;
            append(schema, view, facts, current, next, left)
        }
        Some(tag)
            if tag == SequenceCase::Finish.tag() as i32
                && view.origin(next[2]) == view.origin(current[2])
                && field(
                    view,
                    next[3],
                    schema.queue,
                    SequenceCase::Finish.tag(),
                    q::FINISH_REST,
                    queue,
                )
                && field(
                    view,
                    next[4],
                    schema.queue,
                    SequenceCase::Finish.tag(),
                    q::FINISH_PARENT,
                    queue,
                ) =>
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
        || !field(
            view,
            callee,
            schema.queue,
            SequenceCase::Cons.tag(),
            q::CONS_HEAD,
            queue,
        )
        || !field(
            view,
            answer[0],
            schema.computation,
            ComputationCase::Return.tag(),
            c::PAYLOAD,
            input,
        )
        || !field(
            view,
            rest,
            schema.queue,
            SequenceCase::Cons.tag(),
            q::CONS_TAIL,
            queue,
        )
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
    let right = |value| {
        field(
            view,
            value,
            schema.queue,
            SequenceCase::Append.tag(),
            q::APPEND_RIGHT,
            current[3],
        )
    };
    let same = view.origin(next[2]) == view.origin(current[2]);
    match facts.get(&(schema.queue, view.origin(left))).copied() {
        Some(tag) if tag == SequenceCase::Empty.tag() as i32 && same && right(next[3]) => Ok(()),
        Some(tag) if tag == SequenceCase::Cons.tag() as i32 => {
            let node = view
                .node(next[3], schema.queue, Some(SequenceCase::Append.tag()))
                .filter(|fields| fields.len() == 2)
                .ok_or("Append callback must retain its current right queue")?;
            if !right(node[1]) {
                return Err("Append callback must retain its current right queue");
            }
            callback(schema, view, current[2], left, next[2], node[0])
        }
        Some(tag) if tag == SequenceCase::Append.tag() as i32 && same => {
            let node = view
                .node(next[3], schema.queue, Some(SequenceCase::Append.tag()))
                .filter(|fields| fields.len() == 2)
                .ok_or("Append reassociation must preserve left-to-right queue order")?;
            let tail = view
                .node(node[1], schema.queue, Some(SequenceCase::Append.tag()))
                .filter(|fields| fields.len() == 2)
                .ok_or("Append reassociation must preserve left-to-right queue order")?;
            if field(
                view,
                node[0],
                schema.queue,
                SequenceCase::Append.tag(),
                q::APPEND_LEFT,
                left,
            ) && field(
                view,
                tail[0],
                schema.queue,
                SequenceCase::Append.tag(),
                q::APPEND_RIGHT,
                left,
            ) && right(tail[1])
            {
                Ok(())
            } else {
                Err("Append reassociation must preserve left-to-right queue order")
            }
        }
        Some(tag) if tag == SequenceCase::Finish.tag() as i32 && same => {
            let node = view
                .node(next[3], schema.queue, Some(SequenceCase::Finish.tag()))
                .filter(|fields| fields.len() == 3)
                .ok_or("Append must retain the complete left finish frame")?;
            let tail = view
                .node(node[1], schema.queue, Some(SequenceCase::Append.tag()))
                .filter(|fields| fields.len() == 2)
                .ok_or("Append must retain the complete left finish frame")?;
            if field(
                view,
                node[0],
                schema.queue,
                SequenceCase::Finish.tag(),
                q::FINISH_EXTENT,
                left,
            ) && field(
                view,
                node[2],
                schema.queue,
                SequenceCase::Finish.tag(),
                q::FINISH_PARENT,
                left,
            ) && field(
                view,
                tail[0],
                schema.queue,
                SequenceCase::Finish.tag(),
                q::FINISH_REST,
                left,
            ) && right(tail[1])
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
        || facts.get(&(schema.computation, view.origin(current[2])))
            != Some(&(ComputationCase::Return.tag() as i32))
        || facts.get(&(schema.queue, view.origin(current[3])))
            != Some(&(SequenceCase::Empty.tag() as i32))
        || facts.get(&(schema.context, view.origin(current[5])))
            != Some(&(ContextCase::Root.tag() as i32))
    {
        return Err("execution result must be its terminal root return");
    }
    let fields = view
        .node(result, schema.pair, None)
        .filter(|fields| fields.len() == 2)
        .ok_or("root return must publish its state and current answer")?;
    if view.origin(fields[0]) != view.origin(current[0])
        || !field(
            view,
            fields[1],
            schema.computation,
            ComputationCase::Return.tag(),
            c::PAYLOAD,
            current[2],
        )
    {
        return Err("root return must publish its state and current answer");
    }
    Ok(())
}

/// One required action of the flat driver. Every selection the operational
/// semantics admits has exactly one entry; `ActionRegistry::finish` requires all
/// of them, so a missing case is a completeness failure rather than a silent
/// gap. Actions are recorded only from validated actual exits: a trapping
/// default cannot stand in for a required root or queue action.
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
    pub const ALL: [Self; 8] = [
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

#[derive(Default)]
pub(crate) struct ActionRegistry(std::collections::HashSet<ActionId>);

impl ActionRegistry {
    /// Record the action of one validated actual exit.
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
        let tag = |case: crate::control::schema::ComputationCase| case.tag() as i32;
        match selected(schema.computation, current[2]) {
            Some(computation)
                if computation == tag(ComputationCase::Perform)
                    && selected(schema.context, current[5])
                        == Some(ContextCase::Root.tag() as i32) =>
            {
                self.0.insert(ActionId::PerformRoot);
            }
            Some(computation) if computation == tag(ComputationCase::Return) => {
                match selected(schema.queue, current[3]) {
                    Some(queue)
                        if queue == SequenceCase::Empty.tag() as i32
                            && selected(schema.context, current[5])
                                == Some(ContextCase::Root.tag() as i32) =>
                    {
                        self.0.insert(ActionId::ReturnEmptyRoot);
                    }
                    Some(queue)
                        if queue == SequenceCase::Cons.tag() as i32
                            || queue == SequenceCase::Finish.tag() as i32 =>
                    {
                        self.0.insert(if queue == SequenceCase::Cons.tag() as i32 {
                            ActionId::ReturnCons
                        } else {
                            ActionId::ReturnFinish
                        });
                    }
                    Some(queue) if queue == SequenceCase::Append.tag() as i32 => {
                        if let Ok(left) = find_field(
                            view,
                            current[3],
                            schema.queue,
                            SequenceCase::Append.tag(),
                            q::APPEND_LEFT,
                            facts,
                        ) && let Some(left) = selected(schema.queue, left)
                        {
                            let action = match left {
                                value if value == SequenceCase::Empty.tag() as i32 => {
                                    Some(ActionId::AppendEmpty)
                                }
                                value if value == SequenceCase::Cons.tag() as i32 => {
                                    Some(ActionId::AppendCons)
                                }
                                value if value == SequenceCase::Append.tag() as i32 => {
                                    Some(ActionId::AppendAppend)
                                }
                                value if value == SequenceCase::Finish.tag() as i32 => {
                                    Some(ActionId::AppendFinish)
                                }
                                _ => None,
                            };
                            if let Some(action) = action {
                                self.0.insert(action);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub fn finish(&self) -> Result<(), &'static str> {
        if ActionId::ALL.iter().all(|action| self.0.contains(action)) {
            Ok(())
        } else {
            Err("execution must retain every root and queue action")
        }
    }
}

mod observables;
pub(crate) use observables::verify as effects;

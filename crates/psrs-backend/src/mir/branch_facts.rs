//! Field-zero equality facts propagated across real CFG edges and SSA joins.
use super::value_identity::{Definitions, definitions, reference_origin};
use super::{BlockId, Function, Instruction as I, Terminator as T};
use crate::types::{DefinedTypeId, ValueId};
use std::collections::{HashMap, VecDeque};

pub(crate) type Key = (DefinedTypeId, ValueId);
pub(crate) type Facts = HashMap<Key, i32>;

pub(crate) fn tag_source(value: ValueId, definitions: &Definitions<'_>) -> Option<Key> {
    let origin = |value| reference_origin(value, definitions);
    match definitions.get(&origin(value)).copied()? {
        I::StructGet {
            type_index,
            field: 0,
            value,
            ..
        } => Some((*type_index, origin(*value))),
        _ => None,
    }
}

pub(crate) fn compute(function: &Function) -> HashMap<BlockId, Facts> {
    let definitions = definitions(function);
    let origin = |value| reference_origin(value, &definitions);
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let mut states = HashMap::<BlockId, Facts>::from([(function.entry, Facts::new())]);
    let mut pending = VecDeque::from([function.entry]);
    while let Some(id) = pending.pop_front() {
        let Some(block) = blocks.get(&id) else {
            continue;
        };
        let Some(terminator) = &block.terminator else {
            continue;
        };
        let mut edges = super::cfg::successors(terminator)
            .into_iter()
            .map(|target| (target, states[&id].clone()))
            .collect::<Vec<_>>();
        if let T::Switch { value, cases, .. } = terminator
            && let Some(key) = tag_source(*value, &definitions)
        {
            for (index, (tag, _)) in cases.iter().enumerate() {
                edges[index].1.insert(key, *tag);
            }
            if let Some((_, facts)) = edges.last_mut() {
                facts.remove(&key);
            }
        }
        // A join parameter denotes its incoming argument on this edge. Existing
        // facts about that parameter must not survive a new loop iteration.
        if let T::Jump {
            target, arguments, ..
        } = terminator
            && let Some(successor) = blocks.get(target)
        {
            for (_, facts) in &mut edges {
                let old = facts.clone();
                facts.retain(|(_, value), _| !successor.parameters.contains(value));
                for (parameter, argument) in successor.parameters.iter().zip(arguments) {
                    for ((ty, value), tag) in &old {
                        if *value == origin(*argument) {
                            facts.insert((*ty, *parameter), *tag);
                        }
                    }
                }
            }
        }
        for (target, incoming) in edges {
            if !blocks.contains_key(&target) {
                continue;
            }
            let changed = match states.get_mut(&target) {
                None => {
                    states.insert(target, incoming);
                    true
                }
                Some(existing) => {
                    let before = existing.len();
                    existing.retain(|key, tag| incoming.get(key) == Some(tag));
                    existing.len() != before
                }
            };
            if changed {
                pending.push_back(target);
            }
        }
    }
    states
}

pub(crate) struct Exit {
    pub block: BlockId,
    pub facts: Facts,
    pub aliases: HashMap<ValueId, ValueId>,
    pub points: Vec<(BlockId, usize)>,
    pub rejections: Vec<(ValueId, Vec<i32>, (BlockId, usize))>,
    pub trapped: bool,
    pub selections: Vec<(ValueId, i32, (BlockId, usize))>,
}

/// Enumerate normal exits within one CFG region. An edge to `stop` exits the
/// region; trapping paths have no result. Keep each path's SSA arguments so a
/// shared return block cannot erase which selected frame produced its result.
pub(crate) fn paths(
    function: &Function,
    start: BlockId,
    stop: Option<BlockId>,
) -> Result<Vec<Exit>, &'static str> {
    let definitions = definitions(function);
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let mut pending = vec![(
        start,
        Facts::new(),
        HashMap::new(),
        std::collections::HashSet::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )];
    let mut exits = Vec::new();
    while let Some((id, facts, aliases, mut active, mut points, selections, rejections)) =
        pending.pop()
    {
        if !active.insert(id) {
            return Err("execution region has an internal cycle");
        }
        let block = blocks.get(&id).ok_or("execution path has no block")?;
        points.extend((0..=block.instructions.len()).map(|index| (id, index)));
        let terminator = block
            .terminator
            .as_ref()
            .ok_or("execution path has no exit")?;
        let origin = |value| super::value_identity::path_origin(value, &definitions, &aliases);
        match terminator {
            T::Trap { .. } => exits.push(Exit {
                block: id,
                facts,
                aliases,
                points,
                selections,
                rejections,
                trapped: true,
            }),
            T::Return { .. } | T::ReturnCall { .. } | T::ReturnCallRef { .. } => exits.push(Exit {
                block: id,
                facts,
                aliases,
                points,
                selections,
                rejections,
                trapped: false,
            }),
            T::Jump { target, .. } if Some(*target) == stop => exits.push(Exit {
                block: id,
                facts,
                aliases,
                points,
                selections,
                rejections,
                trapped: false,
            }),
            T::Jump {
                target, arguments, ..
            } => {
                let successor = blocks
                    .get(target)
                    .ok_or("execution path has no successor")?;
                if successor.parameters.len() != arguments.len() {
                    return Err("execution path has invalid incoming arguments");
                }
                let bindings = successor
                    .parameters
                    .iter()
                    .zip(arguments)
                    .map(|(parameter, argument)| (*parameter, origin(*argument)))
                    .collect::<Vec<_>>();
                let mut next = aliases;
                next.extend(bindings);
                pending.push((*target, facts, next, active, points, selections, rejections));
            }
            T::Branch {
                condition,
                then_block,
                else_block,
                ..
            } => {
                for (target, truth) in [(*then_block, true), (*else_block, false)] {
                    let mut selected_values = selections.clone();
                    let mut rejected_values = rejections.clone();
                    if let Some((value, tag)) =
                        equality_selection(*condition, truth, &definitions, &aliases)
                    {
                        selected_values.push((value, tag, (id, block.instructions.len())));
                    } else if let Some((value, tag)) =
                        equality_selection(*condition, !truth, &definitions, &aliases)
                    {
                        rejected_values.push((value, vec![tag], (id, block.instructions.len())));
                    }
                    pending.push((
                        target,
                        facts.clone(),
                        aliases.clone(),
                        active.clone(),
                        points.clone(),
                        selected_values,
                        rejected_values,
                    ));
                }
            }
            T::Switch {
                value,
                cases,
                default,
                ..
            } => {
                let selected = match definitions.get(&origin(*value)).copied() {
                    Some(I::StructGet {
                        type_index,
                        field: 0,
                        value,
                        ..
                    }) => Some((*type_index, origin(*value))),
                    _ => None,
                };
                for (tag, target) in cases {
                    let mut branch = facts.clone();
                    if let Some(key) = selected {
                        branch.insert(key, *tag);
                    }
                    let mut selected_values = selections.clone();
                    selected_values.push((origin(*value), *tag, (id, block.instructions.len())));
                    pending.push((
                        *target,
                        branch,
                        aliases.clone(),
                        active.clone(),
                        points.clone(),
                        selected_values,
                        rejections.clone(),
                    ));
                }
                let mut fallback = facts;
                if let Some(key) = selected {
                    fallback.remove(&key);
                }
                let mut rejections = rejections;
                rejections.push((
                    origin(*value),
                    cases.iter().map(|(tag, _)| *tag).collect(),
                    (id, block.instructions.len()),
                ));
                pending.push((
                    *default, fallback, aliases, active, points, selections, rejections,
                ));
            }
        }
    }
    Ok(exits)
}

/// Normal-result consumers deliberately exclude exceptional paths. Semantic
/// effect verification uses `paths` to retain all operations preceding a trap.
pub(crate) fn exits(
    function: &Function,
    start: BlockId,
    stop: Option<BlockId>,
) -> Result<Vec<Exit>, &'static str> {
    Ok(paths(function, start, stop)?
        .into_iter()
        .filter(|path| !path.trapped)
        .collect())
}

fn equality_selection(
    condition: ValueId,
    truth: bool,
    definitions: &Definitions<'_>,
    aliases: &HashMap<ValueId, ValueId>,
) -> Option<(ValueId, i32)> {
    let origin = |value| super::value_identity::path_origin(value, definitions, aliases);
    let I::Primitive {
        op, left, right, ..
    } = definitions.get(&origin(condition)).copied()?
    else {
        return None;
    };
    if !matches!(
        (op, truth),
        (crate::mir::NumericOp::I32Eq, true) | (crate::mir::NumericOp::I32Ne, false)
    ) {
        return None;
    }
    for (value, constant) in [(*left, *right), (*right, *left)] {
        if let Some(constant) =
            super::value_identity::integer_constant(origin(constant), definitions)
        {
            return Some((origin(value), constant));
        }
    }
    None
}

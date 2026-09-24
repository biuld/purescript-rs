use super::int_range::{
    IntegerRange, arithmetic_range, comparison_range, refine_comparison_false,
    refine_comparison_true,
};
use crate::mir::{BasicBlock, BlockId, Function, Instruction, NumericOp, Terminator, ValueId};
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) type RangeState = HashMap<ValueId, IntegerRange>;

#[derive(Clone, Copy)]
enum EdgeKind<'a> {
    Jump(&'a [ValueId]),
    Branch { condition: ValueId, when_true: bool },
}

#[derive(Clone, Copy)]
struct ControlEdge<'a> {
    source: BlockId,
    target: BlockId,
    kind: EdgeKind<'a>,
    cyclic: bool,
}

struct ControlFlowGraph<'a> {
    edges: Vec<ControlEdge<'a>>,
    outgoing: HashMap<BlockId, Vec<usize>>,
    incoming: HashMap<BlockId, Vec<usize>>,
}

pub(super) type Comparison = (NumericOp, ValueId, ValueId);

pub(super) fn block_entry_ranges(
    function: &Function,
    constants: &HashMap<ValueId, i32>,
) -> HashMap<BlockId, RangeState> {
    let comparisons = comparison_operands(function);
    let graph = control_edges(function);
    let mut edge_states = vec![None; graph.edges.len()];
    let mut entries = HashMap::from([(function.entry, RangeState::new())]);
    let mut pending = VecDeque::from([function.entry]);
    let mut queued = HashSet::from([function.entry]);

    while let Some(block_id) = pending.pop_front() {
        queued.remove(&block_id);
        let Some(entry) = entries.get(&block_id) else {
            continue;
        };
        let Some(block) = function.blocks.iter().find(|block| block.id == block_id) else {
            continue;
        };
        let Some(output) = transfer_block(function, block, entry, constants, &comparisons) else {
            continue;
        };
        for edge_id in graph.outgoing.get(&block_id).into_iter().flatten().copied() {
            let edge = graph.edges[edge_id];
            let next = edge_ranges(function, edge, &output, &comparisons);
            if edge_states[edge_id] == next {
                continue;
            }
            edge_states[edge_id] = next;

            let mut predecessor_states = graph
                .incoming
                .get(&edge.target)
                .into_iter()
                .flatten()
                .filter_map(|predecessor| edge_states[*predecessor].clone())
                .collect::<Vec<_>>();
            if edge.target == function.entry {
                predecessor_states.push(RangeState::new());
            }
            let joined = join_range_states(&predecessor_states);
            if entries.get(&edge.target) == joined.as_ref() {
                continue;
            }
            if let Some(joined) = joined {
                entries.insert(edge.target, joined);
            } else {
                entries.remove(&edge.target);
            }
            if queued.insert(edge.target) {
                pending.push_back(edge.target);
            }
        }
    }
    entries
}

fn control_edges(function: &Function) -> ControlFlowGraph<'_> {
    let mut edges = Vec::new();
    let mut outgoing = HashMap::<BlockId, Vec<usize>>::new();
    let mut incoming = HashMap::<BlockId, Vec<usize>>::new();
    for block in &function.blocks {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        let targets = match terminator {
            Terminator::Return { .. } => Vec::new(),
            Terminator::Jump {
                target, arguments, ..
            } => vec![(*target, EdgeKind::Jump(arguments))],
            Terminator::Branch {
                condition,
                then_block,
                else_block,
                ..
            } => vec![
                (
                    *then_block,
                    EdgeKind::Branch {
                        condition: *condition,
                        when_true: true,
                    },
                ),
                (
                    *else_block,
                    EdgeKind::Branch {
                        condition: *condition,
                        when_true: false,
                    },
                ),
            ],
        };
        for (target, kind) in targets {
            let edge_id = edges.len();
            edges.push(ControlEdge {
                source: block.id,
                target,
                kind,
                cyclic: false,
            });
            outgoing.entry(block.id).or_default().push(edge_id);
            incoming.entry(target).or_default().push(edge_id);
        }
    }
    let adjacency = edges
        .iter()
        .fold(HashMap::<BlockId, Vec<BlockId>>::new(), |mut map, edge| {
            map.entry(edge.source).or_default().push(edge.target);
            map
        });
    for edge in &mut edges {
        edge.cyclic = reaches(edge.target, edge.source, &adjacency);
    }
    ControlFlowGraph {
        edges,
        outgoing,
        incoming,
    }
}

fn reaches(start: BlockId, target: BlockId, adjacency: &HashMap<BlockId, Vec<BlockId>>) -> bool {
    let mut pending = vec![start];
    let mut visited = HashSet::new();
    while let Some(block) = pending.pop() {
        if block == target {
            return true;
        }
        if visited.insert(block) {
            pending.extend(adjacency.get(&block).into_iter().flatten().copied());
        }
    }
    false
}

fn edge_ranges(
    function: &Function,
    edge: ControlEdge<'_>,
    output: &RangeState,
    comparisons: &HashMap<ValueId, Comparison>,
) -> Option<RangeState> {
    let mut ranges = output.clone();
    match edge.kind {
        EdgeKind::Jump(arguments) => {
            let target = function
                .blocks
                .iter()
                .find(|block| block.id == edge.target)?;
            for (parameter, argument) in target.parameters.iter().zip(arguments) {
                if let Some(range) = ranges.get(argument).copied() {
                    ranges.insert(*parameter, range);
                }
            }
        }
        EdgeKind::Branch {
            condition,
            when_true,
        } => {
            let condition_range = ranges.get(&condition).copied()?;
            if let Some(value) = condition_range.exact_value()
                && (value != 0) != when_true
            {
                return None;
            }
            if condition_range.exact_value().is_none()
                && let Some((operation, left, right)) = comparisons.get(&condition)
            {
                let refine = if when_true {
                    refine_comparison_true
                } else {
                    refine_comparison_false
                };
                refine(*operation, *left, *right, &mut ranges);
                normalize_ranges(function, &mut ranges);
            }
        }
    }
    if edge.cyclic {
        return Some(RangeState::new());
    }
    Some(sparse_ranges(function, &ranges))
}

fn transfer_block(
    function: &Function,
    block: &BasicBlock,
    entry: &RangeState,
    constants: &HashMap<ValueId, i32>,
    comparisons: &HashMap<ValueId, Comparison>,
) -> Option<RangeState> {
    let mut ranges = full_ranges(function);
    for (value, constant) in constants {
        ranges.insert(*value, IntegerRange::exact(*constant));
    }
    ranges.extend(entry.iter().map(|(value, range)| (*value, *range)));

    for instruction in &block.instructions {
        match instruction {
            Instruction::TrapIf { condition, .. } => {
                let condition_range = ranges.get(condition).copied()?;
                if condition_range
                    .exact_value()
                    .is_some_and(|value| value != 0)
                {
                    return None;
                }
                if condition_range.exact_value().is_none()
                    && let Some((operation, left, right)) = comparisons.get(condition)
                {
                    refine_comparison_false(*operation, *left, *right, &mut ranges);
                    normalize_ranges(function, &mut ranges);
                }
            }
            Instruction::Constant {
                destination, value, ..
            } => {
                ranges.insert(*destination, IntegerRange::exact(*value));
            }
            Instruction::Copy {
                destination, value, ..
            } => {
                if let Some(range) = ranges.get(value).copied() {
                    ranges.insert(*destination, range);
                }
            }
            Instruction::Primitive {
                destination,
                op,
                left,
                right,
                ..
            } => {
                let (Some(left_range), Some(right_range)) =
                    (ranges.get(left).copied(), ranges.get(right).copied())
                else {
                    continue;
                };
                if is_comparison(*op) {
                    if let Some(range) = comparison_range(*op, left_range, right_range) {
                        ranges.insert(*destination, range);
                    }
                } else if let Some(range) = arithmetic_range(*op, left_range, right_range) {
                    ranges.insert(*destination, range);
                }
            }
            Instruction::LinearLoad { destination, .. } => {
                if let Some(value) = constants.get(destination) {
                    ranges.insert(*destination, IntegerRange::exact(*value));
                }
            }
            _ => {}
        }
    }
    Some(ranges)
}

fn full_ranges(function: &Function) -> RangeState {
    function
        .values
        .iter()
        .filter_map(|value| IntegerRange::for_type(value.ty).map(|range| (value.id, range)))
        .collect()
}

fn sparse_ranges(function: &Function, ranges: &RangeState) -> RangeState {
    let full = function
        .values
        .iter()
        .filter_map(|value| IntegerRange::for_type(value.ty).map(|range| (value.id, range)))
        .collect::<HashMap<_, _>>();
    ranges
        .iter()
        .filter_map(|(value, range)| (full.get(value) != Some(range)).then_some((*value, *range)))
        .collect()
}

fn normalize_ranges(function: &Function, ranges: &mut RangeState) {
    let full = full_ranges(function);
    for (value, range) in ranges {
        if range.minimum > range.maximum
            && let Some(full_range) = full.get(value)
        {
            *range = *full_range;
        }
    }
}

fn join_range_states(states: &[RangeState]) -> Option<RangeState> {
    let mut states = states.iter();
    let mut joined = states.next()?.clone();
    for state in states {
        joined.retain(|value, range| {
            if let Some(next) = state.get(value) {
                range.minimum = range.minimum.min(next.minimum);
                range.maximum = range.maximum.max(next.maximum);
                true
            } else {
                false
            }
        });
    }
    Some(joined)
}

pub(super) fn comparison_operands(function: &Function) -> HashMap<ValueId, Comparison> {
    let mut comparisons = HashMap::new();
    loop {
        let mut changed = false;
        for block in &function.blocks {
            for instruction in &block.instructions {
                match instruction {
                    Instruction::Primitive {
                        destination,
                        op,
                        left,
                        right,
                        ..
                    } if is_comparison(*op) => {
                        if let std::collections::hash_map::Entry::Vacant(entry) =
                            comparisons.entry(*destination)
                        {
                            entry.insert((*op, *left, *right));
                            changed = true;
                        }
                    }
                    Instruction::Copy {
                        destination, value, ..
                    } => {
                        if let Some(comparison) = comparisons.get(value).copied()
                            && let std::collections::hash_map::Entry::Vacant(entry) =
                                comparisons.entry(*destination)
                        {
                            entry.insert(comparison);
                            changed = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        if !changed {
            return comparisons;
        }
    }
}

fn is_comparison(operation: NumericOp) -> bool {
    matches!(
        operation,
        NumericOp::I32Eq
            | NumericOp::I32Ne
            | NumericOp::I32LtS
            | NumericOp::I32LeS
            | NumericOp::I32GtS
            | NumericOp::I32GeS
            | NumericOp::BoolEq
            | NumericOp::BoolNe
    )
}

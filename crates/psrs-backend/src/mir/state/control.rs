//! Source-anchored projection of structured CC choices to actual MIR blocks.
use super::*;
use crate::mir::{BlockId, Terminator};

struct Block<'a> {
    parameters: Vec<cc::ValueId>,
    calls: Vec<Call<'a>>,
    end: Option<End>,
}
struct Call<'a> {
    assignment: &'a cc::Assignment,
    adapter: Option<(psrs_hir::SymbolId, cc::ValueId)>,
}
enum End {
    Branch(cc::ValueId, usize, usize),
    Switch(cc::ValueId, Vec<(i32, usize)>, usize),
    Jump(usize, cc::ValueId),
    Return(cc::ValueId),
    Trap,
}

pub(super) fn verify(
    source: &cc::Function,
    function: &Function,
    table: &cc::RepresentationTable,
    runtime: &[RuntimeInvocation],
    host: &[crate::mir::wit::CallPlan],
) -> Result<(), Vec<BackendError>> {
    let mut host_by_entry = std::collections::HashMap::new();
    let mut adapter_blocks = std::collections::HashSet::new();
    for invocation in host {
        invocation.verify(function)?;
        if host_by_entry
            .insert((invocation.entry(), invocation.start()), invocation)
            .is_some()
        {
            return Err(error(
                function,
                "canonical invocation anchors are duplicated",
            ));
        }
        for block in invocation.created_blocks() {
            if !adapter_blocks.insert(block) {
                return Err(error(function, "canonical invocation blocks overlap"));
            }
        }
    }
    let mut checked_host = std::collections::HashSet::new();
    let mut runtime_by_destination = std::collections::HashMap::new();
    for invocation in runtime {
        if runtime_by_destination
            .insert(invocation.destination(), invocation)
            .is_some()
        {
            return Err(error(
                function,
                "runtime invocation source anchors are duplicated",
            ));
        }
    }
    let never = runtime
        .iter()
        .filter(|invocation| invocation.never_returns())
        .map(|invocation| invocation.destination())
        .collect::<std::collections::HashSet<_>>();
    let mut checked_runtime = std::collections::HashSet::new();
    let states = source
        .values
        .iter()
        .filter(|value| value.ty == cc::ValueShape::State)
        .map(|value| value.id)
        .collect::<std::collections::HashSet<_>>();
    let projected_arguments = |arguments: &[cc::ValueId]| {
        arguments
            .iter()
            .filter(|argument| !states.contains(argument))
            .copied()
            .collect::<Vec<_>>()
    };
    let mut blocks = vec![Block {
        parameters: Vec::new(),
        calls: Vec::new(),
        end: None,
    }];
    let end = sequence(&source.assignments, 0, &mut blocks, table, &never)?;
    if blocks[end].end.is_none() {
        blocks[end].end = Some(End::Return(source.result));
    }
    let mut source_blocks = function
        .blocks
        .iter()
        .filter(|block| !adapter_blocks.contains(&block.id))
        .map(|block| block.id)
        .collect::<Vec<_>>();
    source_blocks.sort_by_key(|block| block.0);
    if function.entry != BlockId(0) || source_blocks.len() != blocks.len() {
        return Err(error(
            function,
            "MIR dependency projection changes checked control flow",
        ));
    }
    for (id, expected) in blocks.iter().enumerate() {
        let mut actual = function
            .blocks
            .iter()
            .find(|block| block.id == source_blocks[id])
            .ok_or_else(|| error(function, "MIR dependency projection loses a control block"))?;
        if actual.parameters != expected.parameters {
            return Err(error(
                function,
                "MIR dependency projection changes block parameters",
            ));
        }
        // Collapse only complete, independently checked canonical adapters.
        // Their internal calls and edges remain verified by the owning plan.
        let mut calls = Vec::new();
        let mut cursor = 0;
        loop {
            if let Some(invocation) = host_by_entry.get(&(actual.id, cursor)) {
                if !checked_host.insert((actual.id, cursor)) {
                    return Err(error(function, "canonical invocation revisits an adapter"));
                }
                let instruction = actual
                    .instructions
                    .get(cursor)
                    .ok_or_else(|| error(function, "canonical invocation has no instruction"))?;
                calls.push((actual.id, cursor, instruction, Some(*invocation)));
                let (exit, offset) = invocation.exit();
                actual = function
                    .blocks
                    .iter()
                    .find(|block| block.id == exit)
                    .ok_or_else(|| error(function, "canonical invocation loses its exit"))?;
                cursor = offset;
                continue;
            }
            let Some(instruction) = actual.instructions.get(cursor) else {
                break;
            };
            if matches!(
                instruction,
                Instruction::Call { .. }
                    | Instruction::CallVoid { .. }
                    | Instruction::CallRef { .. }
                    | Instruction::ClosureCall { .. }
            ) {
                calls.push((actual.id, cursor, instruction, None));
            }
            cursor += 1;
        }
        let valid = match (&expected.end, &actual.terminator) {
            (
                Some(End::Branch(condition, left, right)),
                Some(Terminator::Branch {
                    condition: actual,
                    then_block,
                    else_block,
                    ..
                }),
            ) => {
                condition == actual
                    && Some(then_block) == source_blocks.get(*left)
                    && Some(else_block) == source_blocks.get(*right)
            }
            (
                Some(End::Switch(value, cases, default)),
                Some(Terminator::Switch {
                    value: actual,
                    cases: actual_cases,
                    default: actual_default,
                    ..
                }),
            ) => {
                value == actual
                    && cases.len() == actual_cases.len()
                    && cases.iter().zip(actual_cases).all(
                        |((tag, block), (actual_tag, actual_block))| {
                            tag == actual_tag && Some(actual_block) == source_blocks.get(*block)
                        },
                    )
                    && Some(actual_default) == source_blocks.get(*default)
            }
            (
                Some(End::Jump(target, value)),
                Some(Terminator::Jump {
                    target: actual,
                    arguments,
                    ..
                }),
            ) => Some(actual) == source_blocks.get(*target) && arguments.as_slice() == [*value],
            (Some(End::Return(value)), Some(Terminator::Return { value: actual, .. })) => {
                value == actual
            }
            (Some(End::Trap), Some(Terminator::Trap { .. })) => matches!(
                actual.instructions.last(),
                Some(Instruction::Unreachable { .. })
            ),
            _ => false,
        };
        if !valid {
            return Err(error(
                function,
                "MIR dependency projection changes a branch or successor",
            ));
        }
        if calls.len() != expected.calls.len() {
            return Err(error(
                function,
                "MIR dependency projection adds or loses an invocation",
            ));
        }
        for ((call_block, call_index, instruction, host), call) in calls.iter().zip(&expected.calls)
        {
            let assignment = call.assignment;
            if let Some(invocation) = host {
                let cc::AssignmentKind::DirectCall { arguments, .. } = &assignment.kind else {
                    return Err(error(
                        function,
                        "canonical adapter has no direct source call",
                    ));
                };
                if call.adapter.is_some()
                    || !invocation.matches_source(assignment, &projected_arguments(arguments))
                {
                    return Err(error(
                        function,
                        "canonical adapter changes its source invocation",
                    ));
                }
                continue;
            }
            if let Some(invocation) = runtime_by_destination.get(&assignment.destination) {
                invocation.verify_call(source, assignment, function, *call_block, *call_index)?;
                checked_runtime.insert(assignment.destination);
                continue;
            }
            if instruction.destination() != Some(assignment.destination) {
                return Err(error(
                    function,
                    "MIR dependency projection reorders invocations",
                ));
            }
            let valid = if let Some((expected, value)) = call.adapter {
                matches!(instruction, Instruction::Call { function, arguments, .. }
                    if *function == expected && arguments.as_slice() == [value])
            } else {
                match (&assignment.kind, instruction) {
                    (
                        cc::AssignmentKind::DirectCall {
                            function: expected,
                            arguments: expected_arguments,
                        },
                        Instruction::Call {
                            function: actual,
                            arguments: actual_arguments,
                            ..
                        },
                    ) => {
                        expected == actual
                            && projected_arguments(expected_arguments) == *actual_arguments
                    }
                    (
                        cc::AssignmentKind::IndirectCall {
                            function: expected,
                            arguments: expected_arguments,
                            ..
                        },
                        Instruction::ClosureCall {
                            function: actual,
                            arguments: actual_arguments,
                            ..
                        },
                    ) => {
                        expected == actual
                            && projected_arguments(expected_arguments) == *actual_arguments
                    }
                    (
                        cc::AssignmentKind::StateExecution {
                            function: expected, ..
                        },
                        Instruction::ClosureCall {
                            function: actual,
                            arguments,
                            ..
                        },
                    ) => expected == actual && arguments.is_empty(),
                    _ => false,
                }
            };
            if !valid {
                return Err(error(
                    function,
                    "MIR dependency projection changes an invocation producer or operands",
                ));
            }
        }
    }
    if checked_host.len() != host.len() {
        return Err(error(
            function,
            "canonical invocation evidence has no corresponding source call",
        ));
    }
    if checked_runtime.len() != runtime.len() {
        return Err(error(
            function,
            "runtime invocation evidence has no corresponding source call",
        ));
    }
    Ok(())
}

fn sequence<'a>(
    assignments: &'a [cc::Assignment],
    mut current: usize,
    blocks: &mut Vec<Block<'a>>,
    table: &cc::RepresentationTable,
    never: &std::collections::HashSet<cc::ValueId>,
) -> Result<usize, Vec<BackendError>> {
    for assignment in assignments {
        match &assignment.kind {
            cc::AssignmentKind::DirectCall { .. }
            | cc::AssignmentKind::IndirectCall { .. }
            | cc::AssignmentKind::StateExecution { .. } => {
                blocks[current].calls.push(Call {
                    assignment,
                    adapter: None,
                });
                if never.contains(&assignment.destination) {
                    blocks[current].end = Some(End::Trap);
                    return Ok(current);
                }
            }
            cc::AssignmentKind::If {
                condition,
                then_assignments,
                else_assignments,
                then_value,
                else_value,
            } => {
                let left = blocks.len();
                blocks.extend((0..3).map(|offset| Block {
                    parameters: if offset == 2 {
                        vec![assignment.destination]
                    } else {
                        Vec::new()
                    },
                    calls: Vec::new(),
                    end: None,
                }));
                let right = left + 1;
                let join = left + 2;
                blocks[current].end = Some(End::Branch(*condition, left, right));
                let left_end = sequence(then_assignments, left, blocks, table, never)?;
                let right_end = sequence(else_assignments, right, blocks, table, never)?;
                if blocks[left_end].end.is_none() {
                    blocks[left_end].end = Some(End::Jump(join, *then_value));
                }
                if blocks[right_end].end.is_none() {
                    blocks[right_end].end = Some(End::Jump(join, *else_value));
                }
                current = join;
            }
            cc::AssignmentKind::AggregateConvert {
                value, conversion, ..
            } => {
                let projected =
                    cc::state::StateCallProjection::payload_conversion(conversion, table).ok();
                let conversion = projected.as_ref().unwrap_or(conversion);
                if let cc::ValueConversion::FunctionAdapter { function, .. } = conversion.plan {
                    blocks[current].calls.push(Call {
                        assignment,
                        adapter: Some((function, *value)),
                    });
                }
            }
            cc::AssignmentKind::Unreachable => {
                blocks[current].end = Some(End::Trap);
                return Ok(current);
            }
            cc::AssignmentKind::TagSwitch {
                value,
                cases,
                default_assignments,
                default_value,
            } => {
                let first = blocks.len();
                let default = first + cases.len();
                let join = default + 1;
                blocks.extend((first..=join).map(|id| Block {
                    parameters: if id == join {
                        vec![assignment.destination]
                    } else {
                        Vec::new()
                    },
                    calls: Vec::new(),
                    end: None,
                }));
                blocks[current].end = Some(End::Switch(
                    *value,
                    cases
                        .iter()
                        .enumerate()
                        .map(|(index, case)| (case.tag, first + index))
                        .collect(),
                    default,
                ));
                for (index, case) in cases.iter().enumerate() {
                    let end = sequence(&case.assignments, first + index, blocks, table, never)?;
                    if blocks[end].end.is_none() {
                        blocks[end].end = Some(End::Jump(join, case.value));
                    }
                }
                let end = sequence(default_assignments, default, blocks, table, never)?;
                if blocks[end].end.is_none() {
                    blocks[end].end = Some(End::Jump(join, *default_value));
                }
                current = join;
            }
            _ => {}
        }
    }
    Ok(current)
}

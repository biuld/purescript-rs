//! Static verification for the Canonical ABI's linear-memory accesses.

use crate::BackendError;
use crate::abi::SCRATCH_END;
use crate::mir::{self, Function, Instruction, NumericOp, Terminator};
use crate::types::ValueId;
use std::collections::HashMap;

const WASM32_ADDRESS_SPACE: u64 = 1_u64 << 32;
const PASS: &str = "P10 Wasm structuring";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddressFact {
    Pending,
    Known(u32),
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegionKind {
    Scratch,
    StringLiteral,
}

#[derive(Clone, Copy, Debug)]
struct Region {
    start: u64,
    end: u64,
    kind: RegionKind,
}

impl Region {
    fn writable(self) -> bool {
        self.kind == RegionKind::Scratch
    }
}

#[derive(Clone, Copy, Debug)]
enum AccessKind {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug)]
struct MemoryAccess {
    address: ValueId,
    offset: u32,
    width: u64,
    kind: AccessKind,
    span: psrs_span::TextRange,
}

/// Checks statically knowable MIR memory intervals against the ABI regions.
/// Dynamic reads retain WebAssembly's runtime bounds checks. A dynamic store
/// is rejected because MIR currently carries no writable-buffer proof.
pub(super) fn verify_static_access_extents(
    module: &mir::Module,
    string_offsets: &HashMap<String, u32>,
) -> Result<(), Vec<BackendError>> {
    let regions = build_regions(module, string_offsets)?;
    let mut errors = Vec::new();

    for function in &module.functions {
        let facts = solve_address_facts(function, string_offsets);
        for block in &function.blocks {
            for instruction in &block.instructions {
                let Some(access) = memory_access(instruction) else {
                    continue;
                };
                verify_access(function, access, &facts, &regions, &mut errors);
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn build_regions(
    module: &mir::Module,
    string_offsets: &HashMap<String, u32>,
) -> Result<Vec<Region>, Vec<BackendError>> {
    let mut regions = vec![Region {
        start: 0,
        end: u64::from(SCRATCH_END),
        kind: RegionKind::Scratch,
    }];

    for (bytes, offset) in string_offsets {
        let span = string_span(module, bytes).unwrap_or(module.span);
        let length = u64::try_from(bytes.len()).map_err(|_| {
            vec![extent_error(
                None,
                span,
                "MIR string data segment exceeds the wasm32 address space",
            )]
        })?;
        let start = u64::from(*offset);
        let end = start
            .checked_add(4)
            .and_then(|prefix_end| prefix_end.checked_add(length))
            .filter(|end| *end <= WASM32_ADDRESS_SPACE)
            .ok_or_else(|| {
                vec![extent_error(
                    None,
                    span,
                    "MIR string data segment exceeds the wasm32 address space",
                )]
            })?;
        regions.push(Region {
            start,
            end,
            kind: RegionKind::StringLiteral,
        });
    }

    regions.sort_by_key(|region| region.start);
    for pair in regions.windows(2) {
        if pair[1].start < pair[0].end {
            return Err(vec![extent_error(
                None,
                module.span,
                "MIR ABI memory regions overlap",
            )]);
        }
    }
    Ok(regions)
}

fn string_span(module: &mir::Module, text: &str) -> Option<psrs_span::TextRange> {
    module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::StringConstant { bytes, span, .. } if bytes == text => Some(*span),
            _ => None,
        })
}

fn verify_access(
    function: &Function,
    access: MemoryAccess,
    facts: &HashMap<ValueId, AddressFact>,
    regions: &[Region],
    errors: &mut Vec<BackendError>,
) {
    let Some(fixed_end) = u64::from(access.offset).checked_add(access.width) else {
        errors.push(function_error(
            function,
            access.span,
            "MIR memory access offset and width overflow the verifier range",
        ));
        return;
    };
    if fixed_end > WASM32_ADDRESS_SPACE {
        errors.push(function_error(
            function,
            access.span,
            "MIR memory access offset and width exceed the wasm32 address space",
        ));
        return;
    }

    match facts
        .get(&access.address)
        .copied()
        .unwrap_or(AddressFact::Unknown)
    {
        AddressFact::Known(base) => {
            verify_known_access(function, access, base, regions, errors);
        }
        AddressFact::Pending | AddressFact::Unknown => {
            if matches!(access.kind, AccessKind::Write) {
                errors.push(function_error(
                    function,
                    access.span,
                    "MIR store has no statically known writable ABI region",
                ));
            }
        }
    }
}

fn verify_known_access(
    function: &Function,
    access: MemoryAccess,
    base: u32,
    regions: &[Region],
    errors: &mut Vec<BackendError>,
) {
    let Some(start) = u64::from(base).checked_add(u64::from(access.offset)) else {
        errors.push(function_error(
            function,
            access.span,
            "MIR memory access base and offset overflow the verifier range",
        ));
        return;
    };
    let Some(end) = start.checked_add(access.width) else {
        errors.push(function_error(
            function,
            access.span,
            "MIR memory access extent overflows wasm32 address arithmetic",
        ));
        return;
    };
    if end > WASM32_ADDRESS_SPACE {
        errors.push(function_error(
            function,
            access.span,
            "MIR memory access extent exceeds the wasm32 address space",
        ));
        return;
    }

    let Some(region) = regions
        .iter()
        .find(|region| u64::from(base) >= region.start && u64::from(base) < region.end)
    else {
        errors.push(function_error(
            function,
            access.span,
            "MIR memory access starts outside the canonical ABI regions",
        ));
        return;
    };
    if end > region.end {
        errors.push(function_error(
            function,
            access.span,
            "MIR memory access crosses a canonical ABI region boundary",
        ));
        return;
    }
    if matches!(access.kind, AccessKind::Write) && !region.writable() {
        errors.push(function_error(
            function,
            access.span,
            "MIR store targets a read-only string literal",
        ));
    }
}

fn memory_access(instruction: &Instruction) -> Option<MemoryAccess> {
    match instruction {
        Instruction::Load {
            address,
            offset,
            span,
            ..
        } => Some(MemoryAccess {
            address: *address,
            offset: *offset,
            width: 4,
            kind: AccessKind::Read,
            span: *span,
        }),
        Instruction::Load8U {
            address,
            offset,
            span,
            ..
        } => Some(MemoryAccess {
            address: *address,
            offset: *offset,
            width: 1,
            kind: AccessKind::Read,
            span: *span,
        }),
        Instruction::Store {
            address,
            offset,
            span,
            ..
        } => Some(MemoryAccess {
            address: *address,
            offset: *offset,
            width: 4,
            kind: AccessKind::Write,
            span: *span,
        }),
        _ => None,
    }
}

fn solve_address_facts(
    function: &Function,
    string_offsets: &HashMap<String, u32>,
) -> HashMap<ValueId, AddressFact> {
    let mut facts = function
        .values
        .iter()
        .map(|value| (value.id, AddressFact::Unknown))
        .collect::<HashMap<_, _>>();
    for block in &function.blocks {
        for parameter in &block.parameters {
            facts.insert(*parameter, AddressFact::Pending);
        }
        for instruction in &block.instructions {
            if let Some(destination) = instruction.destination() {
                let fact = match instruction {
                    Instruction::Constant { value, .. } => AddressFact::Known(*value as u32),
                    Instruction::StringConstant { bytes, .. } => string_offsets
                        .get(bytes)
                        .copied()
                        .map(AddressFact::Known)
                        .unwrap_or(AddressFact::Unknown),
                    Instruction::Copy { .. } => AddressFact::Pending,
                    Instruction::Primitive {
                        op: NumericOp::I32Add | NumericOp::I32Sub,
                        ..
                    } => AddressFact::Pending,
                    _ => AddressFact::Unknown,
                };
                facts.insert(destination, fact);
            }
        }
    }

    let block_inputs = block_parameter_inputs(function);
    loop {
        let previous = facts.clone();
        for block in &function.blocks {
            for instruction in &block.instructions {
                let Some(destination) = instruction.destination() else {
                    continue;
                };
                let fact = instruction_fact(instruction, &previous, string_offsets);
                facts.insert(destination, fact);
            }
            for parameter in &block.parameters {
                let incoming = block_inputs
                    .get(parameter)
                    .map(|arguments| {
                        arguments
                            .iter()
                            .map(|argument| {
                                previous
                                    .get(argument)
                                    .copied()
                                    .unwrap_or(AddressFact::Unknown)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                facts.insert(*parameter, join_facts(&incoming));
            }
        }
        if facts == previous {
            break;
        }
    }

    for fact in facts.values_mut() {
        if *fact == AddressFact::Pending {
            *fact = AddressFact::Unknown;
        }
    }
    facts
}

fn block_parameter_inputs(function: &Function) -> HashMap<ValueId, Vec<ValueId>> {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let mut inputs = HashMap::<ValueId, Vec<ValueId>>::new();
    for block in &function.blocks {
        let Some(Terminator::Jump {
            target, arguments, ..
        }) = &block.terminator
        else {
            continue;
        };
        let Some(target_block) = blocks.get(target) else {
            continue;
        };
        for (parameter, argument) in target_block.parameters.iter().zip(arguments) {
            inputs.entry(*parameter).or_default().push(*argument);
        }
    }
    inputs
}

fn instruction_fact(
    instruction: &Instruction,
    facts: &HashMap<ValueId, AddressFact>,
    string_offsets: &HashMap<String, u32>,
) -> AddressFact {
    let fact = |value: ValueId| facts.get(&value).copied().unwrap_or(AddressFact::Unknown);
    match instruction {
        Instruction::Constant { value, .. } => AddressFact::Known(*value as u32),
        Instruction::StringConstant { bytes, .. } => string_offsets
            .get(bytes)
            .copied()
            .map(AddressFact::Known)
            .unwrap_or(AddressFact::Unknown),
        Instruction::Copy { value, .. } => fact(*value),
        Instruction::Primitive {
            op: NumericOp::I32Add,
            left,
            right,
            ..
        } => combine_i32(fact(*left), fact(*right), u32::wrapping_add),
        Instruction::Primitive {
            op: NumericOp::I32Sub,
            left,
            right,
            ..
        } => combine_i32(fact(*left), fact(*right), u32::wrapping_sub),
        _ => AddressFact::Unknown,
    }
}

fn combine_i32(
    left: AddressFact,
    right: AddressFact,
    operation: fn(u32, u32) -> u32,
) -> AddressFact {
    match (left, right) {
        (AddressFact::Known(left), AddressFact::Known(right)) => {
            AddressFact::Known(operation(left, right))
        }
        (AddressFact::Unknown, _) | (_, AddressFact::Unknown) => AddressFact::Unknown,
        _ => AddressFact::Pending,
    }
}

fn join_facts(facts: &[AddressFact]) -> AddressFact {
    if facts.is_empty() {
        return AddressFact::Unknown;
    }
    let mut known = None;
    let mut pending = false;
    for fact in facts {
        match fact {
            AddressFact::Pending => pending = true,
            AddressFact::Unknown => return AddressFact::Unknown,
            AddressFact::Known(value) => match known {
                Some(previous) if previous != *value => return AddressFact::Unknown,
                Some(_) => {}
                None => known = Some(*value),
            },
        }
    }
    if pending {
        AddressFact::Pending
    } else {
        known
            .map(AddressFact::Known)
            .unwrap_or(AddressFact::Unknown)
    }
}

fn function_error(
    function: &Function,
    span: psrs_span::TextRange,
    message: &'static str,
) -> BackendError {
    extent_error(Some(function), span, message)
}

fn extent_error(
    function: Option<&Function>,
    span: psrs_span::TextRange,
    message: &'static str,
) -> BackendError {
    let error = BackendError::new(PASS, span, message);
    match function {
        Some(function) => error.with_module(function.symbol.module),
        None => error,
    }
}

#[cfg(test)]
#[path = "extent_tests.rs"]
mod tests;

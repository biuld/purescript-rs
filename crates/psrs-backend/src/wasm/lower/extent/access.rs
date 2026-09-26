use super::address::AddressFact;
use super::{Region, WASM32_ADDRESS_SPACE, function_error};
use crate::abi::REALLOC_SYMBOL;
use crate::mir::{Function, Instruction, ListDirection};
use crate::types::ValueId;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
enum AccessKind {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MemoryAccess {
    address: ValueId,
    offset: u32,
    width: u64,
    kind: AccessKind,
    span: psrs_span::TextRange,
}

/// A list store writes only inside the buffer `cabi_realloc` just returned.
/// The index is dynamic, so the checker accepts the copy when the pointer is
/// that allocation and does not try to bound each element statically.
pub(super) fn verify_list_copy(
    function: &Function,
    instruction: &Instruction,
    errors: &mut Vec<crate::BackendError>,
) {
    let Instruction::ListCopy {
        direction: ListDirection::Store,
        pointer,
        span,
        ..
    } = instruction
    else {
        return;
    };
    let from_realloc = function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                Instruction::Call {
                    destination,
                    function,
                    ..
                } if *destination == *pointer && *function == REALLOC_SYMBOL
            )
        })
    });
    if !from_realloc {
        errors.push(function_error(
            function,
            *span,
            "MIR list store has no cabi_realloc provenance",
        ));
    }
}

pub(super) fn verify_access(
    function: &Function,
    access: MemoryAccess,
    facts: &HashMap<ValueId, AddressFact>,
    regions: &[Region],
    errors: &mut Vec<crate::BackendError>,
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
        AddressFact::Allocated(size) => {
            if fixed_end > u64::from(size) {
                errors.push(function_error(
                    function,
                    access.span,
                    "MIR memory access exceeds its allocator-provided ABI buffer",
                ));
            }
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
    errors: &mut Vec<crate::BackendError>,
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

pub(super) fn memory_access(instruction: &Instruction) -> Option<MemoryAccess> {
    let (address, offset, span, width, kind) = match instruction {
        Instruction::Load {
            address,
            offset,
            span,
            ..
        } => (*address, *offset, *span, 4, AccessKind::Read),
        Instruction::Load8U {
            address,
            offset,
            span,
            ..
        } => (*address, *offset, *span, 1, AccessKind::Read),
        Instruction::Store {
            address,
            offset,
            span,
            ..
        } => (*address, *offset, *span, 4, AccessKind::Write),
        Instruction::Store8 {
            address,
            offset,
            span,
            ..
        } => (*address, *offset, *span, 1, AccessKind::Write),
        Instruction::Store16 {
            address,
            offset,
            span,
            ..
        } => (*address, *offset, *span, 2, AccessKind::Write),
        Instruction::StoreI64 {
            address,
            offset,
            span,
            ..
        }
        | Instruction::StoreF64 {
            address,
            offset,
            span,
            ..
        } => (*address, *offset, *span, 8, AccessKind::Write),
        Instruction::StoreF32 {
            address,
            offset,
            span,
            ..
        } => (*address, *offset, *span, 4, AccessKind::Write),
        _ => return None,
    };
    Some(MemoryAccess {
        address,
        offset,
        width,
        kind,
        span,
    })
}

//! Static verification for the Canonical ABI's linear-memory accesses.

mod access;
mod address;

use crate::BackendError;
use crate::abi::SCRATCH_END;
use crate::mir::{self, Instruction};
use std::collections::HashMap;

const WASM32_ADDRESS_SPACE: u64 = 1_u64 << 32;
const PASS: &str = "P10 Wasm structuring";

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

/// Checks statically knowable MIR memory intervals against the ABI regions.
/// Dynamic reads retain WebAssembly's runtime bounds checks. Dynamic writes
/// must point into a buffer returned by the ABI allocator.
pub(super) fn verify_static_access_extents(
    module: &mir::Module,
    string_offsets: &HashMap<String, u32>,
) -> Result<(), Vec<BackendError>> {
    let regions = build_regions(module, string_offsets)?;
    let mut errors = Vec::new();

    for function in &module.functions {
        let facts = address::solve_address_facts(function, string_offsets);
        for block in &function.blocks {
            for instruction in &block.instructions {
                let Some(memory_access) = access::memory_access(instruction) else {
                    continue;
                };
                access::verify_access(function, memory_access, &facts, &regions, &mut errors);
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

pub(super) fn function_error(
    function: &mir::Function,
    span: psrs_span::TextRange,
    message: &'static str,
) -> BackendError {
    extent_error(Some(function), span, message)
}

fn extent_error(
    function: Option<&mir::Function>,
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
mod tests;

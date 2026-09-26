//! Static verification for the Canonical ABI's linear-memory accesses.

mod access;
mod address;

use crate::BackendError;
use crate::abi::SCRATCH_END;
use crate::mir;

const WASM32_ADDRESS_SPACE: u64 = 1_u64 << 32;
const PASS: &str = "P10 Wasm structuring";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegionKind {
    Scratch,
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

/// Checks statically knowable MIR memory intervals against the ABI scratch
/// region. Dynamic reads retain WebAssembly's runtime bounds checks. Dynamic
/// writes must point into a buffer returned by the ABI allocator. GC string
/// literals are not MIR-addressable and define no region.
pub(super) fn verify_static_access_extents(module: &mir::Module) -> Result<(), Vec<BackendError>> {
    let regions = build_regions();
    let mut errors = Vec::new();

    for function in &module.functions {
        let facts = address::solve_address_facts(function);
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

fn build_regions() -> Vec<Region> {
    vec![Region {
        start: 0,
        end: u64::from(SCRATCH_END),
        kind: RegionKind::Scratch,
    }]
}

pub(super) fn function_error(
    function: &mir::Function,
    span: psrs_span::TextRange,
    message: &'static str,
) -> BackendError {
    BackendError::new(PASS, span, message).with_module(function.symbol.module)
}

#[cfg(test)]
mod tests;

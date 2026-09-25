use super::{local, value_type, wasm_error};
use crate::BackendError;
use crate::mir::{self, BlockId, Function as MirFunction, Instruction as MirInstruction};
use crate::types::ValueId;
use crate::wasm::convert::heap_type;
use crate::wasm::{Body, Op};
use ops::{memory, memory_with_align, primitive, ref_cast, ref_test};
mod arrays;
mod cfg;
mod closure;
mod dispatcher;
mod helpers;
mod instructions;
#[cfg(test)]
mod irreducible_dispatch_tests;
mod legacy;
mod ops;
mod region;
#[cfg(test)]
mod tests;
mod unary;
use crate::wasm::FunctionIndex;
use closure::ClosureOps;
use dispatcher::DispatcherOps;
use helpers::ValueOps;
use legacy::LegacyRegionOps;
use psrs_hir::SymbolId;
use region::RegionOps;
use std::collections::{HashMap, HashSet};
use wasm_encoder::Instruction;

/// A block whose instructions contain an `Unreachable` never transfers control
/// through its terminator: the trap and everything after it are dead. The
/// structurer must not read the trap destination local, which is never
/// initialized for a reference result.
pub(super) fn block_traps(block: &mir::BasicBlock) -> bool {
    block
        .instructions
        .iter()
        .any(|instruction| matches!(instruction, MirInstruction::Unreachable { .. }))
}

pub(super) struct Structurer<'a> {
    pub(super) function: &'a MirFunction,
    pub(super) blocks: HashMap<BlockId, &'a mir::BasicBlock>,
    pub(super) locals: HashMap<ValueId, u32>,
    pub(super) function_indices: &'a HashMap<SymbolId, FunctionIndex>,
    pub(super) string_offsets: &'a HashMap<String, u32>,
}
impl Structurer<'_> {
    pub(super) fn emit_control_flow(&self, body: &mut Body) -> Result<bool, Vec<BackendError>> {
        match cfg::ControlFlowPlan::build(self.function, &self.blocks)? {
            cfg::ControlFlowPlan::Dispatcher(blocks) => {
                self.emit_dispatcher(&blocks, body)?;
                Ok(true)
            }
            cfg::ControlFlowPlan::Reducible(root) if root.contains_loops() => {
                RegionOps::emit_control_flow(self, &root, body)?;
                Ok(false)
            }
            cfg::ControlFlowPlan::Reducible(_) => {
                LegacyRegionOps::emit_linear_region(
                    self,
                    self.function.entry,
                    None,
                    &mut HashSet::new(),
                    body,
                )?;
                Ok(false)
            }
        }
    }
}

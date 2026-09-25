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
mod helpers;
mod instructions;
mod ops;
mod region;
#[cfg(test)]
mod tests;
mod unary;
use crate::wasm::FunctionIndex;
use closure::ClosureOps;
use helpers::ValueOps;
use psrs_hir::SymbolId;
use region::RegionOps;
use std::collections::HashMap;
use wasm_encoder::Instruction;

pub(super) struct Structurer<'a> {
    pub(super) function: &'a MirFunction,
    pub(super) blocks: HashMap<BlockId, &'a mir::BasicBlock>,
    pub(super) locals: HashMap<ValueId, u32>,
    pub(super) function_indices: &'a HashMap<SymbolId, FunctionIndex>,
    pub(super) string_offsets: &'a HashMap<String, u32>,
}
impl Structurer<'_> {
    pub(super) fn emit_control_flow(&self, body: &mut Body) -> Result<(), Vec<BackendError>> {
        let plan = cfg::ControlFlowPlan::build(self.function, &self.blocks)?;
        RegionOps::emit_control_flow(self, &plan, body)
    }
}

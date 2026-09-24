use super::{local, value_type, wasm_error};
use crate::BackendError;
use crate::mir::{self, BlockId, Function as MirFunction, Instruction as MirInstruction};
use crate::types::ValueId;
use crate::wasm::convert::heap_type;
use crate::wasm::{Body, Op};
use ops::{linear_load, linear_store, memory, memory_with_align, primitive, ref_cast, ref_test};
mod arrays;
mod closure;
mod helpers;
mod instructions;
mod ops;
mod region;
mod unary;
use crate::wasm::FunctionIndex;
use closure::ClosureOps;
use helpers::ValueOps;
use psrs_hir::SymbolId;
use region::RegionOps;
use std::collections::{HashMap, HashSet};
use wasm_encoder::Instruction;

pub(super) fn linear_copy_local_indices(
    function: &MirFunction,
) -> Result<Option<[u32; 3]>, Vec<BackendError>> {
    arrays::linear_copy_local_indices(function)
}

pub(super) struct Structurer<'a> {
    pub(super) function: &'a MirFunction,
    pub(super) blocks: HashMap<BlockId, &'a mir::BasicBlock>,
    pub(super) locals: HashMap<ValueId, u32>,
    pub(super) function_indices: &'a HashMap<SymbolId, FunctionIndex>,
    pub(super) string_offsets: &'a HashMap<String, u32>,
    pub(super) linear_allocator: Option<FunctionIndex>,
    pub(super) linear_copy_locals: Option<[u32; 3]>,
}
impl Structurer<'_> {
    pub(super) fn emit_region(
        &self,
        current: BlockId,
        stop: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
        body: &mut Body,
    ) -> Result<Option<ValueId>, Vec<BackendError>> {
        RegionOps::emit_region(self, current, stop, visited, body)
    }
}

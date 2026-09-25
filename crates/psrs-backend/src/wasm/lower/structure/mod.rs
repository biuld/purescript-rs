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
mod ops;
#[cfg(test)]
mod reducible_tests;
mod region;
#[cfg(test)]
mod switch_tests;
#[cfg(test)]
mod tests;
mod unary;
use crate::wasm::FunctionIndex;
use closure::ClosureOps;
use dispatcher::DispatcherOps;
use helpers::ValueOps;
use psrs_hir::SymbolId;
use region::RegionOps;
use std::collections::HashMap;
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
            cfg::ControlFlowPlan::Reducible(root) => {
                RegionOps::emit_control_flow(self, &root, body)?;
                Ok(false)
            }
        }
    }

    /// Reads a MIR value from its local. Non-parameter reference values are
    /// stored in nullable locals (see `lower_function`), so this restores the
    /// value's non-null type with `ref.as_non_null` at each read. A function
    /// parameter keeps its declared non-null type and needs no cast.
    pub(super) fn emit_load(
        &self,
        value: ValueId,
        span: psrs_span::TextRange,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        body.push(Op::Leaf(Instruction::LocalGet(local(
            &self.locals,
            value,
            span,
        )?)));
        if self.needs_non_null_cast(value) {
            body.push(Op::Leaf(Instruction::RefAsNonNull));
        }
        Ok(())
    }

    fn needs_non_null_cast(&self, value: ValueId) -> bool {
        if self.function.parameters.contains(&value) {
            return false;
        }
        matches!(
            value_type(self.function, value),
            Some(crate::types::ValueType::Ref(reference)) if !reference.nullable
        )
    }
}

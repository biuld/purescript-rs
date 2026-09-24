//! P9 lowering for the non-GC linear-memory representation.

use super::layout::LayoutError;
use super::planner::LinearMemoryLayout;
use super::scalar_helpers::ScalarHelpers;
use super::{BasicBlock, BlockId, Instruction, Terminator};
use crate::BackendError;
use crate::cc::ValueShape;
use crate::types::{TableSlot, ValueDecl, ValueId, ValueType};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use std::collections::HashMap;

mod arrays;
mod assignments;
mod entry;
mod helpers;
mod variant;
pub(super) use entry::lower_function;

fn layout_error(span: TextRange, error: LayoutError) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        format!("invalid linear-memory layout request: {error:?}"),
    )]
}

fn unsupported(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P9 MIR lowering", span, message)]
}

pub(super) struct LinearFunctionLowerer<'a> {
    pub(super) next_block: u32,
    pub(super) blocks: Vec<BasicBlock>,
    pub(super) values: Vec<ValueDecl>,
    pub(super) shapes: HashMap<ValueId, ValueShape>,
    pub(super) next_value: u32,
    pub(super) layout: &'a LinearMemoryLayout,
    pub(super) wit_imports: &'a HashMap<SymbolId, crate::abi::BoundWasiImport>,
    pub(super) scalar_helpers: &'a ScalarHelpers,
    pub(super) table_slots: &'a HashMap<SymbolId, TableSlot>,
}

impl LinearFunctionLowerer<'_> {
    pub(super) fn fresh(&mut self, ty: ValueType) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        self.values.push(ValueDecl { id, ty });
        id
    }

    pub(super) fn shape(
        &self,
        value: ValueId,
        span: TextRange,
    ) -> Result<ValueShape, Vec<BackendError>> {
        self.shapes
            .get(&value)
            .copied()
            .ok_or_else(|| unsupported(span, "linear-memory value has no abstract shape"))
    }
}

//! Per-function Wasm lowering: local allocation and the list-copy loop locals.

pub(super) mod types;

use super::structure::{self, Structurer};
use super::{defaultable_local_type, local_indices, value_type, wasm_error};
use crate::BackendError;
use crate::mir::Function as MirFunction;
use crate::types::ValueType;
use crate::wasm::convert::val_type;
use crate::wasm::{Body, Function, FunctionIndex, TypeIndex};
use psrs_hir::SymbolId;
use std::collections::HashMap;
use wasm_encoder::ValType;

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_function(
    source: &MirFunction,
    type_index: TypeIndex,
    function_indices: &HashMap<SymbolId, FunctionIndex>,
    string_lengths: &HashMap<crate::types::DataId, u32>,
    literal_globals: &HashMap<crate::types::DataId, crate::wasm::GlobalIndex>,
    static_globals: &HashMap<SymbolId, crate::wasm::GlobalIndex>,
    layout: Option<&crate::mir::layout::PlannedLayout>,
    tags: &[crate::mir::Tag],
    budget: Option<&super::budget::Budget>,
) -> Result<Function, Vec<BackendError>> {
    let locals = local_indices(source)?;
    let list_depth = structure::list_copy_depth(source);
    let list_locals = (list_depth > 0).then(|| {
        let index_base = source.values.len() as u32;
        ListLocals {
            index_base,
            scratch_base: index_base + list_depth,
            array_base: index_base + 2 * list_depth,
            depth: list_depth,
        }
    });
    // EH-14: one fresh `i32` watermark local per try region, allocated after
    // the lowered values and the list-copy locals. A function with try regions
    // never allocates a dispatcher state local (an irreducible CFG with a try
    // region is rejected at `ControlFlowPlan::build`), so the two extra-local
    // families never share an index.
    let watermark_base = source.values.len() as u32
        + list_locals.map_or(0, |locals| {
            locals.index_count() + locals.scratch_count() + locals.array_count()
        });
    let watermark_count = source
        .blocks
        .iter()
        .filter(|block| matches!(&block.terminator, Some(crate::mir::Terminator::Try { .. })))
        .count();
    let parameters = source
        .parameters
        .iter()
        .map(|parameter| {
            value_type(source, *parameter)
                .map(val_type)
                .ok_or_else(|| wasm_error(source.span, "MIR function parameter has no value type"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let local_types = source
        .values
        .iter()
        .skip(source.parameters.len())
        .map(|value| defaultable_local_type(val_type(value.ty)))
        .collect::<Vec<_>>();
    let structurer = Structurer {
        function: source,
        blocks: source
            .blocks
            .iter()
            .map(|block| (block.id, block))
            .collect(),
        locals,
        function_indices,
        string_lengths,
        literal_globals,
        static_globals,
        list_locals,
        layout,
        tags,
        budget: budget.copied(),
        watermark_base,
    };
    let mut body = Body::new();
    // EH-13: the budget prologue precedes the function's first structured
    // instruction, and its overflow throw unwinds out of the function because
    // no try region of this function is armed yet.
    if let Some(budget) = budget {
        budget.increment(source.frame_weight, &mut body);
        budget.overflow_check(&mut body, source.span);
    }
    let uses_dispatcher = structurer.emit_control_flow(&mut body)?;
    structurer.emit_load(source.result, source.span, &mut body)?;
    let mut locals = local_types;
    if let Some(list) = list_locals {
        for _ in 0..list.index_count() {
            locals.push(ValType::I32);
        }
        for _ in 0..list.scratch_count() {
            locals.push(ValType::I32);
        }
        for _ in 0..list.array_count() {
            locals.push(val_type(ValueType::Ref(crate::types::RefType {
                nullable: true,
                heap: crate::types::HeapType::Array,
            })));
        }
    }
    if uses_dispatcher {
        locals.push(ValType::I32);
    }
    // EH-14: the try regions' fresh watermark locals close the local list.
    for _ in 0..watermark_count {
        locals.push(ValType::I32);
    }
    Ok(Function {
        symbol: source.symbol,
        name: source.name.clone(),
        type_index,
        parameters,
        locals,
        body,
        span: source.span,
    })
}

/// The Wasm locals the list-copy loops use. Each nesting level of a non-byte
/// list element gets one `i32` index local, one `i32` scratch local, and one
/// nullable array-reference local, so a `list<list<T>>` copy nests two loops
/// without reusing an index or a temporary.
#[derive(Clone, Copy, Debug)]
pub(super) struct ListLocals {
    index_base: u32,
    scratch_base: u32,
    array_base: u32,
    pub(super) depth: u32,
}

impl ListLocals {
    pub(super) fn index(&self, depth: u32) -> u32 {
        self.index_base + depth
    }

    pub(super) fn scratch(&self, depth: u32) -> u32 {
        self.scratch_base + depth
    }

    pub(super) fn array(&self, depth: u32) -> u32 {
        self.array_base + depth
    }

    pub(super) fn index_count(&self) -> u32 {
        self.depth
    }

    pub(super) fn scratch_count(&self) -> u32 {
        self.depth
    }

    pub(super) fn array_count(&self) -> u32 {
        self.depth
    }
}

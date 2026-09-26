//! `cabi_post_<name>` synthesis for guest exports.
//!
//! `wasi:cli/run` returns a scalar and has no post-return. An export whose flat
//! result is a single owned handle gets a `(i32) -> ()` function that releases
//! the handle with `resource.drop`. An export whose result is lifted through a
//! canonical return area gets a `(i32) -> ()` function that reads the flattened
//! result out of the return area, frees the result's data buffer through
//! `cabi_realloc`, then frees the return area itself.
//! See `docs/design/backend/wasm/canonical-abi-and-wit.md` and
//! `docs/design/backend/wasm/canonical-buffer-allocation-and-lifetime.md`.

use super::super::{
    Export, ExportIndex, ExportKind, FuncType, Function, FunctionIndex, Op, TypeIndex,
};
use super::asm::{Asm, constant, get, memarg, set};
use psrs_hir::SymbolId;
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};

/// One guest export whose flat result is a single owned handle.
pub(crate) struct OwnedHandleExport {
    pub core_name: String,
    pub drop_import: FunctionIndex,
    pub symbol: SymbolId,
}

/// One guest export whose result is a non-scalar lifted through a canonical
/// return area. The core function returns the return-area pointer; the
/// synthesized post-return receives that same pointer.
pub(crate) struct BufferExport {
    pub core_name: String,
    pub symbol: SymbolId,
    /// Alignment requested when the result's data buffer was allocated. It is
    /// only used to satisfy the `realloc` free contract, so any power of two
    /// works; byte buffers use `1`.
    pub buffer_align: u32,
    /// Byte size of the flattened return area, written by the export and freed
    /// by the post-return.
    pub return_area_size: u32,
    /// Alignment of the flattened return area, matching the export's
    /// allocation.
    pub return_area_align: u32,
}

/// Post-returns for exports that return `own<T>`. `wasi:cli/run` is not in
/// this list: its result is a scalar.
pub(crate) fn append_owned_handle_post_returns(
    exports: &[OwnedHandleExport],
    type_index: TypeIndex,
    first_function: FunctionIndex,
    span: TextRange,
) -> Vec<(FuncType, Function, Export)> {
    exports
        .iter()
        .enumerate()
        .map(|(offset, export)| {
            synthesize_owned_handle_post_return(
                &export.core_name,
                type_index,
                FunctionIndex(first_function.0 + offset as u32),
                export.drop_import,
                export.symbol,
                span,
            )
        })
        .collect()
}

/// Legacy core name of the post-return export for `core_export`.
pub(crate) fn post_return_name(core_export: &str) -> String {
    format!("cabi_post_{core_export}")
}

/// A `(i32) -> ()` function that drops the owned handle the export returned.
pub(crate) fn synthesize_owned_handle_post_return(
    core_export: &str,
    type_index: TypeIndex,
    function_index: FunctionIndex,
    drop_import: FunctionIndex,
    symbol: SymbolId,
    span: TextRange,
) -> (FuncType, Function, Export) {
    let signature = FuncType {
        parameters: vec![ValType::I32],
        results: Vec::new(),
    };
    let function = Function {
        symbol,
        name: post_return_name(core_export),
        type_index,
        parameters: vec![ValType::I32],
        locals: Vec::new(),
        body: vec![
            Op::Leaf(Instruction::LocalGet(0)),
            Op::Leaf(Instruction::Call(drop_import.0)),
        ],
        span,
    };
    let export = Export {
        name: post_return_name(core_export),
        kind: ExportKind::Function,
        index: ExportIndex::Function(function_index),
    };
    (signature, function, export)
}

/// Post-returns for exports whose result is lifted through a canonical return
/// area. `wasi:cli/run` is not in this list: its result is a scalar.
pub(crate) fn append_buffer_post_returns(
    exports: &[BufferExport],
    type_index: TypeIndex,
    first_function: FunctionIndex,
    realloc_import: FunctionIndex,
    span: TextRange,
) -> Vec<(FuncType, Function, Export)> {
    exports
        .iter()
        .enumerate()
        .map(|(offset, export)| {
            synthesize_buffer_post_return(
                export,
                type_index,
                FunctionIndex(first_function.0 + offset as u32),
                realloc_import,
                span,
            )
        })
        .collect()
}

/// A `(i32 ret_area) -> ()` function that reclaims an export result.
///
/// The export returns a pointer to a return area holding the flattened result.
/// For a `string`/`list` result the return area is a `(data_ptr, length)` pair.
/// The function frees the data buffer, then frees the return area, both through
/// `cabi_realloc(ptr, len, align, 0)`. The two regions were allocated by the
/// export with the alignments recorded in `export`, so the free contract holds.
pub(crate) fn synthesize_buffer_post_return(
    export: &BufferExport,
    type_index: TypeIndex,
    function_index: FunctionIndex,
    realloc_import: FunctionIndex,
    span: TextRange,
) -> (FuncType, Function, Export) {
    const RET_AREA: u32 = 0;
    const DATA_PTR: u32 = 1;
    const DATA_LEN: u32 = 2;

    let mut asm = Asm::new();
    // The flattened result starts at the returned return-area pointer.
    get(&mut asm, RET_AREA);
    asm.leaf(Instruction::I32Load(memarg(0)));
    set(&mut asm, DATA_PTR);
    get(&mut asm, RET_AREA);
    asm.leaf(Instruction::I32Load(memarg(4)));
    set(&mut asm, DATA_LEN);

    // Free the result's data buffer.
    get(&mut asm, DATA_PTR);
    get(&mut asm, DATA_LEN);
    constant(&mut asm, export.buffer_align as i32);
    constant(&mut asm, 0);
    asm.leaf(Instruction::Call(realloc_import.0));
    asm.leaf(Instruction::Drop);

    // Free the return area itself.
    get(&mut asm, RET_AREA);
    constant(&mut asm, export.return_area_size as i32);
    constant(&mut asm, export.return_area_align as i32);
    constant(&mut asm, 0);
    asm.leaf(Instruction::Call(realloc_import.0));
    asm.leaf(Instruction::Drop);

    let signature = FuncType {
        parameters: vec![ValType::I32],
        results: Vec::new(),
    };
    let function = Function {
        symbol: export.symbol,
        name: post_return_name(&export.core_name),
        type_index,
        parameters: vec![ValType::I32],
        locals: vec![ValType::I32; 2],
        body: asm.into_body(),
        span,
    };
    let export = Export {
        name: post_return_name(&export.core_name),
        kind: ExportKind::Function,
        index: ExportIndex::Function(function_index),
    };
    (signature, function, export)
}

/// The production lowering has no export with a non-scalar result yet, so no
/// post-return is synthesized. Keeping the two synthesis entry points reachable
/// from here means the export mechanism can populate the descriptor lists
/// without introducing dead code.
pub(crate) fn assert_no_known_post_returns(span: TextRange) {
    debug_assert!(
        append_owned_handle_post_returns(&[], TypeIndex(0), FunctionIndex(0), span).is_empty()
    );
    debug_assert!(
        append_buffer_post_returns(&[], TypeIndex(0), FunctionIndex(0), FunctionIndex(0), span)
            .is_empty()
    );
}

#[cfg(test)]
mod tests;

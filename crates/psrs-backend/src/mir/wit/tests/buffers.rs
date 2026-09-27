use super::common::{RecordingLowerer, signature};
use super::*;
use crate::abi;
use crate::abi::canonical::CanonicalType;
use crate::cc::ValueShape;
use psrs_hir::{ModuleId, SymbolId};

fn import(name: &str, params: Vec<CanonicalType>, result: Option<CanonicalType>) -> WasiImport {
    crate::abi::test_support::import(
        SymbolId::new(ModuleId(0), 0),
        "test:interface",
        name,
        params,
        result,
    )
}

/// Whether the lowering ends with `cabi_realloc(ptr, len, 1, 0)`: the two
/// constants and the four-argument call.
fn ends_with_buffer_free(instructions: &[Instruction]) -> bool {
    let count = instructions.len();
    let number = |index: usize, expected: i32| {
        matches!(
            instructions.get(index),
            Some(Instruction::Constant { value, .. }) if *value == expected
        )
    };
    number(count - 3, 1)
        && number(count - 2, 0)
        && matches!(
            instructions.get(count - 1),
            Some(Instruction::Call { function, arguments, .. })
                if *function == abi::REALLOC_SYMBOL && arguments.len() == 4
        )
}

fn assert_ends_with_free(instructions: &[Instruction]) {
    assert!(
        ends_with_buffer_free(instructions),
        "the lowering must free its transient buffer: {instructions:?}"
    );
}

#[test]
fn list_results_free_the_import_buffer_after_decoding() {
    let import = import("bytes", Vec::new(), Some(CanonicalType::String));
    let mut lowerer = RecordingLowerer::default();
    lower(
        &mut lowerer,
        &import,
        &signature(Vec::new()),
        ValueId(0),
        &[],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("a list result should lower through the codec and free path");

    assert_ends_with_free(&lowerer.instructions);
}

#[test]
fn string_arguments_free_the_transcode_buffer_after_the_call() {
    let import = import("log", vec![CanonicalType::String], None);
    let mut lowerer = RecordingLowerer::default();
    lower(
        &mut lowerer,
        &import,
        &signature(vec![ValueShape::String]),
        ValueId(0),
        &[ValueId(1)],
        TextRange::new(0, 1),
        BlockId(0),
    )
    .expect("a string argument should lower through the codec and free path");

    assert_ends_with_free(&lowerer.instructions);
}

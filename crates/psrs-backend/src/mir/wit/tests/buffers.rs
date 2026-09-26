use super::common::{RecordingLowerer, signature};
use super::*;
use crate::abi::{self, WasiParamKind, WasiResultKind};
use crate::cc::ValueShape;
use psrs_hir::{ModuleId, SymbolId};

fn import(
    name: &str,
    parameters: Vec<ValueType>,
    param_kinds: Vec<WasiParamKind>,
    result: Option<ValueType>,
    result_kind: WasiResultKind,
    retptr: bool,
) -> WasiImport {
    WasiImport {
        symbol: SymbolId::new(ModuleId(0), 0),
        module: "test:interface".into(),
        name: name.into(),
        parameters,
        param_kinds,
        result,
        result_kind,
        unsupported: None,
        retptr,
        flat_slots: Vec::new(),
    }
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
    let import = import(
        "bytes",
        Vec::new(),
        Vec::new(),
        None,
        WasiResultKind::List,
        true,
    );
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
    let import = import(
        "log",
        vec![ValueType::I32, ValueType::I32],
        vec![WasiParamKind::List],
        None,
        WasiResultKind::None,
        false,
    );
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

use super::*;

#[test]
fn bounds_cover_exported_entry_points_and_every_reachable_private_function() {
    for (export, call_private, export_table, accepted) in [
        (0, false, false, true),
        (1, false, false, false),
        (0, true, false, false),
        (0, false, true, false),
    ] {
        let bytes = entry_module(export, call_private, export_table);
        assert_eq!(measure_stack_bound(&bytes, 0).is_ok(), accepted);
    }
}

fn entry_module(export: u32, call_private: bool, export_table: bool) -> Vec<u8> {
    use wasm_encoder::*;
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    functions.function(0);
    module.section(&functions);
    let mut tables = TableSection::new();
    tables.table(TableType {
        element_type: RefType::FUNCREF,
        minimum: 1,
        maximum: Some(1),
        table64: false,
        shared: false,
    });
    module.section(&tables);
    let mut exports = ExportSection::new();
    exports.export("entry", ExportKind::Func, export);
    if export_table {
        exports.export("table", ExportKind::Table, 0);
    }
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut entry = Function::new([]);
    if call_private {
        entry.instruction(&Instruction::Call(1));
    }
    entry.instruction(&Instruction::End);
    code.function(&entry);
    let mut private = Function::new([]);
    private.instruction(&Instruction::I32Const(0));
    private.instruction(&Instruction::CallIndirect {
        type_index: 0,
        table_index: 0,
    });
    private.instruction(&Instruction::End);
    code.function(&private);
    module.section(&code);
    module.finish()
}

#[test]
fn the_runtime_artifact_has_a_measured_static_bound() {
    let bound = measure_stack_bound(psrs_runtime::NUMBER_RUNTIME.bytes, 0)
        .expect("the numeric runtime is nonrecursive");
    assert_eq!(
        bound.bytes,
        psrs_runtime::NUMBER_RUNTIME.storage.stack_bound_bytes,
        "the declared stack bound must match the static analysis"
    );
}

#[test]
fn a_recursive_graph_is_rejected() {
    // Two mutually recursive functions, each reserving a frame.
    let bytes = recursive_module();
    assert!(
        measure_stack_bound(&bytes, 0)
            .unwrap_err()
            .contains("recursive call graph")
    );
}

#[test]
fn unrecognized_stack_writes_and_unbalanced_frames_are_rejected() {
    use wasm_encoder::Instruction as I;
    let prologue = [
        I::GlobalGet(0),
        I::I32Const(32),
        I::I32Sub,
        I::LocalTee(0),
        I::GlobalSet(0),
    ];
    for suffix in [
        vec![I::I32Const(1), I::GlobalSet(0)],
        vec![I::Return],
        vec![I::Br(0)],
        vec![I::I32Const(9), I::LocalSet(0)],
        vec![I::GlobalGet(0), I::I32Const(32), I::I32Sub, I::GlobalSet(0)],
        vec![],
    ] {
        let mut ops = prologue.to_vec();
        ops.extend(suffix);
        assert!(measure_stack_bound(&single_function(&ops), 0).is_err());
    }
    assert!(measure_stack_bound(&single_function(&[I::I32Const(5), I::GlobalSet(0)]), 0).is_err());
}

#[test]
fn a_reference_branch_cannot_bypass_an_otherwise_valid_restoration() {
    use wasm_encoder::Instruction as I;
    let ops = [
        I::GlobalGet(0),
        I::I32Const(32),
        I::I32Sub,
        I::LocalTee(0),
        I::GlobalSet(0),
        I::RefNull(wasm_encoder::HeapType::FUNC),
        I::BrOnNull(0),
        I::Drop,
        I::LocalGet(0),
        I::I32Const(32),
        I::I32Add,
        I::GlobalSet(0),
    ];
    let bytes = single_function(&ops);
    wasmparser::Validator::new()
        .validate_all(&bytes)
        .expect("valid module");
    assert!(
        measure_stack_bound(&bytes, 0)
            .unwrap_err()
            .contains("branch bypasses")
    );
}

fn single_function(ops: &[wasm_encoder::Instruction<'_>]) -> Vec<u8> {
    use wasm_encoder::*;
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(1024),
    );
    module.section(&globals);
    let mut function = Function::new([(1, ValType::I32)]);
    for op in ops {
        function.instruction(op);
    }
    function.instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    module.section(&code);
    module.finish()
}

fn recursive_module() -> Vec<u8> {
    use wasm_encoder::{
        CodeSection, Function, FunctionSection, GlobalSection, GlobalType, Instruction, Module,
        TypeSection, ValType,
    };
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], []);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    functions.function(0);
    module.section(&functions);
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(1024),
    );
    module.section(&globals);
    let mut code = CodeSection::new();
    let mut first = Function::new([]);
    first.instruction(&Instruction::GlobalGet(0));
    first.instruction(&Instruction::I32Const(32));
    first.instruction(&Instruction::I32Sub);
    first.instruction(&Instruction::GlobalSet(0));
    first.instruction(&Instruction::Call(1));
    first.instruction(&Instruction::GlobalGet(0));
    first.instruction(&Instruction::I32Const(32));
    first.instruction(&Instruction::I32Add);
    first.instruction(&Instruction::GlobalSet(0));
    first.instruction(&Instruction::End);
    code.function(&first);
    let mut second = Function::new([]);
    second.instruction(&Instruction::GlobalGet(0));
    second.instruction(&Instruction::I32Const(32));
    second.instruction(&Instruction::I32Sub);
    second.instruction(&Instruction::GlobalSet(0));
    second.instruction(&Instruction::Call(0));
    second.instruction(&Instruction::GlobalGet(0));
    second.instruction(&Instruction::I32Const(32));
    second.instruction(&Instruction::I32Add);
    second.instruction(&Instruction::GlobalSet(0));
    second.instruction(&Instruction::End);
    code.function(&second);
    module.section(&code);
    module.finish()
}

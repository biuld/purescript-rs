use super::*;

#[test]
fn compiles_a_direct_call_with_integer_arithmetic_to_valid_wasm_and_wat() {
    let source = "module Main where\nadd x y = x + y\nmain = add 40 2\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert_eq!(&artifact.wasm[..8], b"\0asm\x0d\0\x01\0");
    assert!(artifact.wat.contains("(component"));
    assert!(artifact.wat.contains("i32.add"));
}

#[test]
fn compiles_character_literals_as_integer_valued_scalars() {
    let source = "module Main where\nchoose :: Char -> Int\nchoose x = 42\nmain = choose 'A'\n";
    let mir = lower_source_to_mir(source);
    let has_character_value = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .any(|instruction| {
            matches!(
                instruction,
                psrs_backend::mir::Instruction::Constant { value: 65, .. }
            )
        });
    assert!(has_character_value);
}

#[test]
fn compiles_number_literals_as_f64_scalars() {
    let source = "module Main where\nchoose :: Number -> Int\nchoose x = 42\nmain = choose 1.5\n";
    let mir = lower_source_to_mir(source);
    let has_number_value = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .any(|instruction| {
            matches!(
                instruction,
                psrs_backend::mir::Instruction::NumberConstant { value, .. } if value == "1.5"
            )
        });
    assert!(has_number_value);
}

#[test]
fn captures_number_values_in_closures() {
    let source = "module Main where\n\
        use :: Number -> Int\n\
        use x = 42\n\
        apply f y = f y\n\
        make :: Number -> Number -> Int\n\
        make x y = apply (\\z -> use x) y\n\
        main = make 1.5 2.0\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("struct.new"));
    assert!(artifact.wat.contains("f64.const"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn compiles_if_expression_through_cfg_to_structured_wasm() {
    let source =
        "module Main where\nchoose condition = if condition then 9 else 2\nmain = choose true\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("if (result i32)"));
    assert!(artifact.wasm.len() > 8);
}

#[test]
fn folds_top_level_scalar_references_to_constants() {
    let source = "module Main where\nanswer = 40\nmain = answer + 2\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("i32.const 42"));
}

#[test]
fn exposes_readable_core_and_backend_ir_dumps() {
    let source = "module Main where\nmain = 42\n";
    let compilation = compile_source_with_dumps("Main.purs", source).unwrap();
    for stage in ["core", "cc", "mir"] {
        assert!(
            compilation
                .dumps
                .get(stage)
                .is_some_and(|dump| !dump.is_empty())
        );
    }
    assert!(compilation.dumps.get("wasm").is_none());
}

#[test]
fn backend_stages_expose_core_after_p7() {
    let core = lower_source_to_core("Main.purs", "module Main where\nmain = 1 + 2\n").unwrap();
    let stages = psrs_backend::compile_with_stages(core).unwrap();
    let main = stages
        .core
        .declarations
        .iter()
        .find(|declaration| declaration.name == "main")
        .expect("linked Core retains main");
    assert_eq!(main.value.kind, psrs_core::ExprKind::Integer(3));
}

#[test]
fn emits_a_wasi_command_component() {
    let source = "module Main where\nmain = 7\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("(component"));
    assert!(artifact.wat.contains("wasi:cli/run@0.2.12"));
    assert!(artifact.wat.contains("wasi:cli/exit@0.2.12"));
}

#[test]
fn typechecks_a_value_imported_from_another_module() {
    let a = ("A.purs", "module A where\nanswer :: Int\nanswer = 40\n");
    let b = ("B.purs", "module B where\nimport A\nmain = answer + 2\n");
    assert!(typecheck_program_sources(&[a, b]).is_ok());
}

#[test]
fn reports_a_cross_module_type_mismatch() {
    let a = (
        "A.purs",
        "module A where\nanswer :: String\nanswer = \"no\"\n",
    );
    let b = ("B.purs", "module B where\nimport A\nmain = answer + 2\n");
    let errors = typecheck_program_sources(&[a, b]).unwrap_err();
    assert!(!errors.is_empty());
}

#[test]
fn compiles_a_value_imported_from_another_module() {
    let a = ("A.purs", "module A where\nanswer :: Int\nanswer = 40\n");
    let b = ("B.purs", "module B where\nimport A\nmain = answer + 2\n");
    let artifact = compile_program_sources(&[a, b]).unwrap();
    assert!(artifact.wat.contains("i32.const 42"));
}

#[test]
fn gives_generated_functions_unique_symbols_across_linked_modules() {
    let a = (
        "A.purs",
        "module A where\nmakeA :: Int -> Int\nmakeA x = (\\ignored -> x) 0\n",
    );
    let b = (
        "B.purs",
        "module B where\nmakeB :: Int -> Int\nmakeB x = (\\ignored -> x) 0\n",
    );
    assert_eq!(a.1.find('\\'), b.1.find('\\'));
    let main = (
        "Main.purs",
        "module Main where\nimport A\nimport B\nmain = makeA 11 + makeB 22\n",
    );
    let core = lower_program_to_core(&[a, b, main]).expect("linking generated functions");
    let stages = psrs_backend::compile_with_stages(core).expect("lowering generated functions");
    let symbols = stages
        .cc
        .functions
        .iter()
        .map(|function| function.symbol)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(symbols.len(), stages.cc.functions.len());
    let Some(output) = run_program_with_wasmtime(&[a, b, main]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(33));
}

#[test]
fn compiles_multiple_user_sources_with_the_embedded_prelude() {
    let helper = ("Helper.purs", "module Helper where\nanswer = 40\n");
    let main = (
        "Main.purs",
        "module Main where\nimport Helper\nimport Prelude\nimport WASI.Console\nmain = let ignored = runEffect (log \"linked\") in 0\n",
    );
    let artifact = compile_program_sources_with_prelude(&[helper, main]).unwrap();
    assert!(artifact.wat.contains("wasi:cli/stdout@0.2.12"));
}

#[test]
fn rejects_ambiguous_program_entries_instead_of_using_source_order() {
    let a = ("A.purs", "module A where\nmain = 1\n");
    let b = ("B.purs", "module B where\nmain = 2\n");
    let errors = compile_program_sources(&[a, b]).unwrap_err();
    assert_eq!(errors.len(), 2);
    assert!(errors.iter().all(|error| {
        error.diagnostic.stage == "P7 entry selection"
            && error
                .diagnostic
                .message
                .contains("multiple `main` declarations")
    }));
}

#[test]
fn attributes_backend_errors_to_their_declaring_module() {
    let a = (
        "A.purs",
        "module A where\nidentity :: forall a. a -> a\nidentity value = value\nmain = identity (\\value -> value) 42\n",
    );
    let b = ("B.purs", "module B where\nanswer = 0\n");
    let errors = compile_program_sources(&[a, b]).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.source == 0
            && error.diagnostic.stage == "P8 closure conversion"
            && error
                .diagnostic
                .message
                .contains("call expects 1 arguments but received 2")
    }));
}

#[test]
fn runs_a_linked_program_when_wasmtime_is_available() {
    let a = ("A.purs", "module A where\nanswer :: Int\nanswer = 40\n");
    let b = ("B.purs", "module B where\nimport A\nmain = answer + 2\n");
    let Some(output) = run_program_with_wasmtime(&[a, b]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

fn run_program_with_wasmtime(sources: &[(&str, &str)]) -> Option<std::process::Output> {
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        return None;
    }
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let artifact = compile_program_sources(sources).unwrap();
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("psrs-{}-{id}.wasm", std::process::id()));
    std::fs::write(&path, &artifact.wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Some(output)
}

use super::*;
use crate::program::lower_program_to_core;

mod effects;

fn lower_source_to_mir(source: &str) -> psrs_backend::mir::Module {
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let backend_input = psrs_backend::cc::lower_module(core).expect("Core should lower to CC");
    psrs_backend::mir::lower_module_with_bindings(
        backend_input.cc,
        backend_input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .expect("CC should lower to MIR")
    .0
}

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
        make :: Number -> Number -> Int\n\
        make x y = (\\z -> use x) y\n\
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

#[test]
fn lowers_string_log_to_wasi_stdout() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let ignored = runEffect (log \"hello world\") in 0\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("wasi:cli/stdout@0.2.12"));
    assert!(artifact.wat.contains("wasi:io/streams@0.2.12"));
    assert!(artifact.wat.contains("hello world"));
    assert!(artifact.wat.contains("i32.load8_u"));
    assert!(artifact.wat.contains("unreachable"));
}

#[test]
fn lowers_a_source_foreign_import_with_a_wit_binding() {
    let source = "module Main where\n\
        foreign import \"wasi:clocks/monotonic-clock#now\" clock :: Int\n\
        main = clock * 0\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("wasi:clocks/monotonic-clock@0.2.12"));
    assert!(artifact.wat.contains("i32.wrap_i64"));
}

#[test]
fn lowers_a_boolean_wit_result_with_a_boolean_source_type() {
    let source = "module Main where\n\
        foreign import \"wasi:io/poll#[method]pollable.ready\" ready :: Int -> Boolean\n\
        main = if ready 0 then 1 else 0\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a Boolean WIT result");
    assert!(artifact.wat.contains("wasi:io/poll@0.2.12"));
    assert!(artifact.wat.contains("call"));
}

#[test]
fn runs_main_as_a_wasi_component_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime("module Main where\nmain = 42\n") else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn prints_hello_world_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(
        "module Main where\nimport Prelude\nimport WASI.Console\nmain = let ignored = runEffect (log \"hello world\") in 0\n",
    ) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"hello world\n");
}

#[test]
fn reads_the_monotonic_clock_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(
        "module Main where\nimport Prelude\nimport WASI.Clock\nmain = (runEffect now) * 0\n",
    ) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn writes_to_stderr_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(
        "module Main where\nimport Prelude\nimport WASI.Console\nmain = let x = runEffect (error \"oops\") in 7\n",
    ) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stderr, b"oops\n");
}

#[test]
fn lowers_a_list_returning_import_with_an_allocator() {
    let source = "module Main where\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: Int -> String\n\
        main = let bytes = randomBytes 8 in 0\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("wasi:random/random@0.2.12"));
    assert!(artifact.wat.contains("cabi_realloc"));
    assert!(artifact.wat.contains("i64.extend_i32_u"));
}

#[test]
fn rejects_a_non_byte_wit_list_before_lowering_it_as_a_string() {
    let source = "module Main where\n\
        foreign import \"wasi:cli/environment#get-arguments\" args :: String\n\
        main = 0\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.stage == "P9 MIR lowering" && error.message.contains("non-byte WIT list results")
    }));
}

#[test]
fn rejects_a_wit_import_when_the_declared_source_type_does_not_match() {
    let source = "module Main where\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: String -> String\n\
        main = 0\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.stage == "P9 MIR lowering" && error.message.contains("incompatible type")
    }));
}

#[test]
fn rejects_a_wasi_interface_outside_the_component_capability_profile() {
    let source = "module Main where\n\
        foreign import \"wasi:random/insecure#get-insecure-random-u64\" random :: Int\n\
        main = 0\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.stage == "P9 MIR lowering"
            && error
                .message
                .contains("not in the current component capability profile")
    }));
}

#[test]
fn reads_random_bytes_when_wasmtime_is_available() {
    let source = "module Main where\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: Int -> String\n\
        main = let bytes = randomBytes 8 in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn passes_a_returned_wit_string_to_another_import() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.Console\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: Int -> String\n\
        main = let ignored = runEffect (log (randomBytes 8)) in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert!(output.status.success(), "wasmtime failed: {output:?}");
    assert_eq!(output.stdout.len(), 9);
}

#[test]
fn keeps_multiple_returned_wit_strings_in_distinct_allocations() {
    let source = "module Main where\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: Int -> String\n\
        main = let first = randomBytes 8 in let second = randomBytes 8 in 0\n";
    let artifact = compile_source("Main.purs", source).expect("lowering repeated list results");
    assert!(artifact.wat.matches("cabi_realloc").count() >= 1);
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
}

fn run_with_wasmtime(source: &str) -> Option<std::process::Output> {
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        return None;
    }
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let artifact = compile_source("Main.purs", source).unwrap();
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

#[test]
fn structures_wasm_ir_with_an_explicit_if_region() {
    let source =
        "module Main where\nchoose condition = if condition then 9 else 2\nmain = choose true\n";
    let core = lower_source_to_core("Main.purs", source).unwrap();
    let stages = psrs_backend::compile_with_stages(core).unwrap();
    assert!(
        stages.wasm.functions.iter().any(|function| function
            .body
            .iter()
            .any(|op| matches!(op, psrs_backend::wasm::Op::If { .. }))),
        "expected a structured if region in the Wasm IR"
    );
}

#[test]
fn reports_type_errors_with_source_ranges() {
    let source = "module Main where\nmain = if 1 then 2 else 3\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(errors.iter().any(|error| error.stage == "P5 typecheck"));
    let integer_offset = source.find("1 then").unwrap() as u32;
    assert!(
        errors
            .iter()
            .any(|error| error.span == TextRange::new(integer_offset, integer_offset + 1))
    );
}

#[test]
fn resolves_a_program_that_imports_a_value_across_modules() {
    let library = "module Library where\nanswer = 42\n";
    let main = "module Main where\nimport Library\nmain = answer\n";
    let modules =
        resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)]).unwrap();
    assert_eq!(modules.len(), 2);
    assert_eq!(modules[1].name, "Main");
    assert_eq!(modules[1].imports.len(), 1);
    assert_eq!(modules[1].imports[0].symbols.len(), 1);
    assert_eq!(modules[1].imports[0].symbols[0].local_name, "answer");
}

#[test]
fn reports_a_missing_imported_module() {
    let main = "module Main where\nimport Missing\nmain = 1\n";
    let errors = check_program(&[("Main.purs", main)]).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].source, 0);
    assert!(errors[0].diagnostic.message.contains("`Missing`"));
}

#[test]
fn reports_an_unknown_export_name() {
    let library = "module Library (answer, missing) where\nanswer = 42\n";
    let errors = resolve_program_sources(&[("Library.purs", library)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.message.contains("`missing`"))
    );
}

#[test]
fn uses_an_explicit_export_list_as_the_module_interface() {
    let library = "module Library (visible) where\nvisible = 1\nhidden = 2\n";
    let main = "module Main where\nimport Library\nmain = hidden\n";
    let errors =
        resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)]).unwrap_err();
    assert!(errors.iter().any(|error| error.source == 1));
}

#[test]
fn reports_orphan_type_declaration_code() {
    let source = "module Main where\nfn :: Int\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OrphanTypeDeclaration"))
    );
}

#[test]
fn reports_orphan_kind_declaration_code() {
    let source = "module Main where\ntype Foo :: Type\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OrphanKindDeclaration"))
    );
}

#[test]
fn reports_overlapping_argument_names_code() {
    let source = "module Main where\nf x x = x\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("OverlappingArgNames"))
    );
}

#[test]
fn reports_duplicate_value_declaration_code() {
    let source = "module Main where\nfoo = 1\nfoo = 2\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("DuplicateValueDeclaration"))
    );
}

#[test]
fn resolves_data_constructors_and_user_types() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nmain = Just 1\n";
    let modules = resolve_program_sources(&[("Main.purs", source)]).unwrap();
    assert_eq!(modules[0].types.len(), 1);
    assert_eq!(modules[0].types[0].constructors.len(), 2);
    let just = modules[0].types[0]
        .constructors
        .iter()
        .find(|constructor| constructor.name == "Just")
        .unwrap()
        .symbol;
    let psrs_hir::ExprKind::Application(function, _) = &modules[0].declarations[0].value.kind
    else {
        panic!("expected a constructor application");
    };
    assert_eq!(function.kind, psrs_hir::ExprKind::Global(just));
}

#[test]
fn resolves_a_user_type_in_a_signature() {
    let source = "module Main where\ndata Maybe a = Nothing\nvalue :: Maybe Int\nvalue = Nothing\n";
    let modules = resolve_program_sources(&[("Main.purs", source)]).unwrap();
    let type_id = modules[0].types[0].id;
    let signature = modules[0].declarations[0].signature.as_ref().unwrap();
    let psrs_hir::TypeKind::Application(function, _) = &signature.kind else {
        panic!("expected an applied type constructor");
    };
    assert_eq!(function.kind, psrs_hir::TypeKind::Named(type_id));
}

#[test]
fn reports_decl_conflict_for_duplicate_type_names() {
    let source = "module Main where\ndata Fail\ndata Fail\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("DeclConflict"))
    );
}

#[test]
fn reports_decl_conflict_between_a_class_and_a_type() {
    let source = "module Main where\nclass Fail\ndata Fail\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("DeclConflict"))
    );
}

#[test]
fn imports_a_type_and_its_constructors_across_modules() {
    let library = "module Library where\ndata Maybe a = Nothing | Just a\n";
    let main = "\
module Main where
import Library (Maybe(..))
value :: Maybe Int
value = Just 1
";
    let modules =
        resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)]).unwrap();
    assert_eq!(modules[1].imports[0].types.len(), 1);
    let type_id = modules[0].types[0].id;
    let signature = modules[1].declarations[0].signature.as_ref().unwrap();
    let psrs_hir::TypeKind::Application(function, _) = &signature.kind else {
        panic!("expected an applied imported type");
    };
    assert_eq!(function.kind, psrs_hir::TypeKind::Named(type_id));
}

#[test]
fn importing_a_type_without_constructors_hides_them() {
    let library = "module Library where\ndata Maybe a = Nothing | Just a\n";
    let main = "module Main where\nimport Library (Maybe)\nvalue = Just 1\n";
    let errors =
        resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.message.contains("`Just`"))
    );
}

#[test]
fn reports_an_unknown_imported_data_constructor() {
    let library = "module Library where\ndata Maybe a = Nothing | Just a\n";
    let main = "module Main where\nimport Library (Maybe(Nope))\nvalue = 1\n";
    let errors =
        resolve_program_sources(&[("Library.purs", library), ("Main.purs", main)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("UnknownImportDataConstructor"))
    );
}

#[test]
fn reports_a_transitive_export_of_an_unexported_type() {
    let source = "module Main (Y(..)) where\ntype X = Int\ndata Y = Y X\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TransitiveExportError"))
    );
}

#[test]
fn reports_a_partial_constructor_export() {
    let source = "module Main (T(A)) where\ndata T = A | B\n";
    let errors = check_program_lenient(&[("Main.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TransitiveDctorExportError"))
    );
}

#[test]
fn reports_an_unknown_exported_data_constructor() {
    let source = "module M1 (X(Y)) where\ndata X = X\ndata Y = Y\n";
    let errors = check_program_lenient(&[("M1.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("UnknownExportDataConstructor"))
    );
}

#[test]
fn reports_a_class_member_export_without_its_class() {
    let source = "module Test (bar) where\nclass Foo a where\n  bar :: a -> a\n";
    let errors = check_program_lenient(&[("Test.purs", source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TransitiveExportError"))
    );
}

#[test]
fn reexports_a_modules_values() {
    let library = "module A where\nx = 1\n";
    let facade = "module Main (module A) where\nimport A\n";
    let user = "module User where\nimport Main\ny = x\n";
    let modules = resolve_program_sources(&[
        ("A.purs", library),
        ("Main.purs", facade),
        ("User.purs", user),
    ])
    .unwrap();
    assert_eq!(modules[1].name, "Main");
    assert!(
        modules[1]
            .exports
            .as_ref()
            .unwrap()
            .values
            .iter()
            .any(|v| v.name == "x")
    );
}

#[test]
fn reports_export_conflict_between_aliased_reexports() {
    let a = "module A where\nx :: Int\nx = 1\n";
    let b = "module B where\nx :: Int\nx = 2\n";
    let c = "module C (module A, module B) where\nimport A as A\nimport B as B\n";
    let errors =
        resolve_program_sources(&[("A.purs", a), ("B.purs", b), ("C.purs", c)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("ExportConflict"))
    );
}

#[test]
fn reports_a_scope_conflict_for_ambiguous_unaliased_reexports() {
    let a = "module A where\nthing = 1\n";
    let b = "module B where\nthing = 2\n";
    let c = "module Main (module A, module B) where\nimport A\nimport B\n";
    let errors =
        resolve_program_sources(&[("A.purs", a), ("B.purs", b), ("Main.purs", c)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("ScopeConflict"))
    );
}

#[test]
fn reports_a_scope_conflict_for_a_duplicated_qualifier() {
    let a = "module A where\nthing = 1\n";
    let b = "module B where\nthing = 2\n";
    let c = "module Main (module X) where\nimport A as X\nimport B as X\n";
    let errors =
        resolve_program_sources(&[("A.purs", a), ("B.purs", b), ("Main.purs", c)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("ScopeConflict"))
    );
}

fn kind_codes(source: &str) -> Vec<&'static str> {
    check_program_kinds_lenient(&[("Main.purs", source)])
        .err()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|error| error.diagnostic.code)
        .collect()
}

#[test]
fn reports_a_kinds_do_not_unify_error() {
    let codes = kind_codes("module Main where\ndata KindError f a = One f | Two (f a)\n");
    assert!(codes.contains(&"KindsDoNotUnify"), "{codes:?}");
}

#[test]
fn reports_a_partially_applied_synonym() {
    let codes = kind_codes("module Main where\ntype F x y = x -> y\ntype G x = F x\n");
    assert!(codes.contains(&"PartiallyAppliedSynonym"), "{codes:?}");
}

#[test]
fn reports_an_infinite_kind() {
    let codes = kind_codes("module Main where\ndata F a = F (a a)\n");
    assert!(codes.contains(&"InfiniteKind"), "{codes:?}");
}

#[test]
fn reports_a_type_synonym_cycle() {
    let codes = kind_codes("module Main where\ntype T = T\n");
    assert!(codes.contains(&"CycleInTypeSynonym"), "{codes:?}");
}

#[test]
fn reports_a_kind_declaration_cycle() {
    let codes = kind_codes(
        "module Main where\ndata Foo :: Bar -> Type\ndata Foo a = Foo\ndata Bar :: Foo -> Type\ndata Bar a = Bar\n",
    );
    assert!(codes.contains(&"CycleInKindDeclaration"), "{codes:?}");
}

#[test]
fn reports_an_undefined_type_variable() {
    let codes = kind_codes("module Main where\nfoo :: Array a\nfoo = 1\n");
    assert!(codes.contains(&"UndefinedTypeVariable"), "{codes:?}");
}

#[test]
fn accepts_well_kinded_higher_kinded_declarations() {
    let codes = kind_codes("module Main where\ndata Compose f g a = Compose (f (g a))\n");
    assert!(codes.is_empty(), "{codes:?}");
}

mod typecheck;

mod adts;

mod arrays;

mod records;

mod functions;

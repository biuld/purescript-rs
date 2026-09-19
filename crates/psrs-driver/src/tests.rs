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
fn compiles_if_expression_through_cfg_to_structured_wasm() {
    let source = "module Main where\nmain = if true then 9 else 2\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("if (result i32)"));
    assert!(artifact.wasm.len() > 8);
}

#[test]
fn lowers_top_level_scalar_references_to_direct_calls() {
    let source = "module Main where\nanswer = 40\nmain = answer + 2\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("call 0"));
    assert!(artifact.wat.contains("i32.add"));
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
fn lowers_string_log_to_wasi_stdout() {
    let source = "module Main where\nmain = log \"hello world\"\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("wasi:cli/stdout@0.2.12"));
    assert!(artifact.wat.contains("wasi:io/streams@0.2.12"));
    assert!(artifact.wat.contains("hello world"));
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
fn runs_main_as_a_wasi_component_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime("module Main where\nmain = 42\n") else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn prints_hello_world_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime("module Main where\nmain = log \"hello world\"\n") else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"hello world\n");
}

#[test]
fn reads_the_monotonic_clock_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime("module Main where\nmain = now * 0\n") else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn writes_to_stderr_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime("module Main where\nmain = let x = error \"oops\" in 7\n")
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stderr, b"oops\n");
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
    let source = "module Main where\nmain = if true then 9 else 2\n";
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

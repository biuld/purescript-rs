//! Official API execution through the locked package and selective native EH.
use super::*;
use psrs_backend::wasm::{CatchClause, FuncType, Op, TagDef, TagIndex, TypeIndex};
use wasm_encoder::{Instruction as I, ValType};

fn source(name: &str) -> String {
    let root = crate::standard_library_info().expect("stdlib package").root;
    std::fs::read_to_string(
        root.join("conformance/effect-exception")
            .join(format!("{name}.purs")),
    )
    .expect("the package contains the public exception cases")
}
fn execute(name: &str, exit: i32, stdout: &[u8]) {
    let Some(output) = run_program_with_prelude_wasmtime(&[("Main.purs", &source(name))]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(exit), "{name}: {output:?}");
    assert_eq!(output.stdout, stdout, "{name}: {output:?}");
    assert!(output.stderr.is_empty(), "{name}: {output:?}");
}
#[test]
fn official_error_public_api_and_stackless_show_execute() {
    execute("PublicApi", 42, b"");
    execute("ShowNameStack", 42, b"");
}
#[test]
fn catches_rethrow_and_unsafe_source_errors() {
    execute("ThrowCatch", 42, b"");
    execute("Unsafe", 42, b"");
}
#[test]
fn error_actions_are_inert_and_repeated_runs_preserve_output_and_mutation_order() {
    execute(
        "Ordering",
        0,
        b"inert\nbody\nhandler\nbody\nhandler\nordered\n",
    );
}
#[test]
fn official_monad_error_effect_instance_executes() {
    execute("MonadError", 42, b"");
}
#[test]
fn source_errors_compose_with_control_and_typed_except() {
    execute("ControlCompose", 42, b"");
}
#[test]
fn uncaught_source_errors_and_uncaught_exhaustion_exit_one() {
    execute("Uncaught", 1, b"");
    execute("StackExhaustion", 1, b"");
}
#[test]
fn source_exception_calls_reject_a_disabled_exception_profile() {
    let errors = crate::program::compile_program_sources_with_prelude_target(
        &[("Main.purs", &source("ThrowCatch"))],
        psrs_backend::TargetCapabilities {
            exceptions: false,
            ..Default::default()
        },
    )
    .expect_err("source exception operations require EH");
    assert!(
        errors
            .iter()
            .any(|e| e.diagnostic.message.contains("exception-handling proposal")),
        "{errors:?}"
    );
}
#[test]
fn malformed_unused_source_exception_declarations_are_rejected_before_erasure() {
    for declaration in [
        "foreign import \"psrs:runtime-exception#throw\" bad :: Int -> Int",
        "foreign import \"psrs:runtime-exception#catch\" bad :: (Unit -> Int) -> (String -> String) -> Int",
        "foreign import \"psrs:runtime-exception#catch\" bad :: (Unit -> Int) -> (Int -> Int) -> Int",
    ] {
        let source =
            format!("module Main where\nimport Prelude\n{declaration}\nmain :: Int\nmain = 42\n");
        let errors =
            crate::program::compile_program_sources_with_prelude(&[("Main.purs", &source)])
                .expect_err("even an unused runtime declaration must satisfy the checked protocol");
        assert!(
            errors.iter().any(|e| e.diagnostic.stage.starts_with("P8")
                && e.diagnostic.message.contains("runtime-exception")),
            "{errors:?}"
        );
    }
}

#[test]
fn an_unrelated_native_tag_bypasses_source_catch_with_payload_intact() {
    if wasmtime_available().is_none() {
        return;
    }
    let source = r#"module Main where
import Prelude
import Effect.Exception as E
import Effect.Unsafe (unsafePerformEffect)
force :: Int -> Int
force n = if n == 0 then 0 else 1 + force (n - 1)
main :: Int
main = unsafePerformEffect (E.catchException (\_ -> pure 99) do
  _ <- pure unit
  pure (force 100000))
"#;
    let mut stages = crate::compile_main_stages(source).expect("source catch verifies");
    let original = psrs_backend::wasm::encode_module(&stages.wasm).unwrap();
    let module = &mut stages.wasm;
    let tag = TagIndex(module.tags.len() as u32);
    let type_index = TypeIndex(module.defined_type_count() + module.types.len() as u32);
    module.types.push(FuncType {
        parameters: vec![ValType::I32],
        results: vec![],
    });
    module.tags.push(TagDef {
        type_index,
        span: module.span,
    });
    let leaf = module
        .functions
        .iter_mut()
        .find(|f| f.name == "force")
        .expect("fallible leaf survives");
    leaf.body = vec![Op::Leaf(I::I32Const(42)), Op::Leaf(I::Throw(tag.0))];
    let main = module
        .functions
        .iter_mut()
        .find(|f| Some(f.symbol) == stages.mir.entry)
        .unwrap();
    let mut protected = std::mem::take(&mut main.body);
    protected.push(Op::Leaf(I::Return));
    main.body = vec![Op::Block {
        body: vec![
            Op::Try {
                clauses: vec![CatchClause::Catch { tag, label: 0 }],
                body: protected,
                span: main.span,
            },
            Op::Leaf(I::Unreachable),
        ],
        result: Some(ValType::I32),
        span: main.span,
    }];
    let core = psrs_backend::wasm::encode_module(module).unwrap();
    let bytes =
        super::exceptions_as_effects::tagged::replace_core(&stages.artifact.wasm, &original, &core);
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .unwrap();
    let output = run_artifact_with_wasmtime(&bytes);
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
}

#[test]
fn assertion_traps_bypass_source_exception_recovery() {
    let source = r#"module Main where
import Prelude
import Effect (Effect)
import Effect.Exception as E
import Effect.Unsafe (unsafePerformEffect)
import Test.Assert (assert')
main :: Int
main = unsafePerformEffect (E.catchException (\_ -> pure 99) do
  assert' "trap" false
  pure 42)
"#;
    let Some(output) = run_program_with_prelude_wasmtime(&[("Main.purs", source)]) else {
        return;
    };
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("wasm trap: wasm `unreachable` instruction executed"),
        "{output:?}"
    );
}

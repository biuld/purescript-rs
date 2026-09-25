use super::*;

#[test]
fn constructs_and_matches_a_concrete_array_in_a_generic_adt_field() {
    let source = "\
module Main where
data Wrap a = Wrap (Array a)
wrap :: Array Int -> Wrap Int
wrap values = Wrap values
unwrap :: Wrap Int -> Int
unwrap value = case value of
  Wrap values -> arrayIndex values 1
main = unwrap (wrap [40, 42])
";
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let stages = psrs_backend::compile_with_stages(core)
        .expect("generic ADT array fields should lower through the erased storage boundary");
    let erased = psrs_backend::cc::ValueShape::Reference(psrs_backend::cc::Reference {
        nullable: false,
        heap: psrs_backend::cc::RefShape::Erased,
    });
    assert!(stages.cc.representations.representations.iter().any(|repr| {
        matches!(repr, psrs_backend::cc::Representation::Variant { cases } if cases.iter().any(|case| case.fields.contains(&erased)))
    }));
    let artifact = stages.artifact;
    assert!(artifact.wat.contains("struct.new"));
    assert!(artifact.wat.contains("ref.cast"));
    let Some(output) = run_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

fn run_wasmtime(source: &str) -> Option<std::process::Output> {
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
fn rejects_generic_adt_array_recovery_before_runtime_cast() {
    let source = "\
module Main where
data Wrap a = Wrap (Array a)
wrap :: Array Int -> Wrap Int
wrap values = Wrap values
unwrap :: forall a. Wrap a -> Int
unwrap (Wrap xs) = arrayLength xs
main = unwrap (wrap [40, 42])
";
    assert_generic_recovery_diagnostic(source, "unwrap", "(Wrap xs)", "generic array");
}

fn assert_generic_recovery_diagnostic(
    source: &str,
    target_function: &str,
    span_text: &str,
    family: &str,
) {
    let start = source
        .find(span_text)
        .expect("diagnostic source fragment should be present") as u32;
    let expected_span = psrs_span::TextRange::new(start, start + span_text.len() as u32);
    let core = lower_source_to_core("GenericRecovery.purs", source)
        .expect("generic recovery fixtures should pass frontend checking");
    assert!(
        core.declarations
            .iter()
            .any(|declaration| declaration.name == target_function),
        "target function {target_function} must reach Typed Core; declarations: {:?}",
        core.declarations
            .iter()
            .map(|declaration| declaration.name.as_str())
            .collect::<Vec<_>>()
    );
    let names = core
        .declarations
        .iter()
        .map(|declaration| declaration.name.clone())
        .collect::<Vec<_>>();
    let errors = match psrs_backend::cc::lower_module(core) {
        Ok(backend) => panic!(
            "generic nominal field recovery should be rejected in CC; declarations={names:?}; functions={:?}",
            backend
                .cc
                .functions
                .iter()
                .map(|function| &function.name)
                .collect::<Vec<_>>()
        ),
        Err(errors) => errors,
    };
    assert!(
        errors.iter().any(|error| {
            error.pass == "P8 closure conversion"
                && error.span == expected_span
                && error.message.contains("unsupported generic")
                && error.message.contains(family)
        }),
        "expected a source-spanned generic {family} recovery diagnostic at {expected_span:?}; got {errors:#?}"
    );
}

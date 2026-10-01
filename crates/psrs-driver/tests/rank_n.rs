use std::process::Command;

#[path = "support/rank_n/mod.rs"]
mod cases;

#[test]
fn source_rank_n_acceptance_and_rejection() {
    let mut failures = Vec::new();
    if let Err(errors) = psrs_driver::check_program(cases::LINKED) {
        failures.push(format!("linked: {errors:?}"));
    }
    for (name, source) in cases::ACCEPT.iter().chain(cases::CHECK_ONLY) {
        if let Err(errors) = psrs_driver::check_source(name, source) {
            failures.push(format!("{name}: {errors:?}"));
        }
    }
    for (name, source) in cases::REJECT {
        if psrs_driver::check_source(name, source).is_ok() {
            failures.push(format!("{name}: accepted an invalid higher-rank program"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn executes_rank_n_values_at_distinct_instantiations() {
    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert_ne!(
            std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(),
            Ok("1"),
            "rank-N runtime acceptance requires wasmtime"
        );
        eprintln!("skipping rank-N execution: wasmtime is unavailable");
        return;
    }
    let mut failures = Vec::new();
    for (name, source) in cases::ACCEPT {
        match psrs_driver::compile_source(name, source) {
            Ok(artifact) => {
                if let Err(error) = expect_execution(name, &artifact.wasm) {
                    failures.push(error);
                }
            }
            Err(errors) => failures.push(format!("{name}: {errors:?}")),
        }
    }
    // The intrinsic indexer is target-specific; CHECK_ONLY compares the
    // annotated polymorphic element boundary with the official compiler.
    let array_source = r#"module Main where
ids :: Array (forall a. a -> a)
ids = [\x -> x]
main :: Int
main = let f = arrayIndex ids 0 in if f true then f 42 else 1
"#;
    match psrs_driver::compile_source("array_elements", array_source) {
        Ok(artifact) => {
            if let Err(error) = expect_execution("array_elements", &artifact.wasm) {
                failures.push(error);
            }
        }
        Err(errors) => failures.push(format!("array_elements: {errors:?}")),
    }
    match psrs_driver::compile_program_sources(cases::LINKED) {
        Ok(artifact) => {
            if let Err(error) = expect_execution("linked", &artifact.wasm) {
                failures.push(error);
            }
        }
        Err(errors) => failures.push(format!("linked: {errors:?}")),
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn expect_execution(name: &str, wasm: &[u8]) -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("psrs-rank-n-{}-{name}.wasm", std::process::id()));
    std::fs::write(&path, wasm).expect("write rank-N component");
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("execute rank-N component");
    let _ = std::fs::remove_file(path);
    if output.status.code() == Some(42) {
        Ok(())
    } else {
        Err(format!("{name}: {output:?}"))
    }
}

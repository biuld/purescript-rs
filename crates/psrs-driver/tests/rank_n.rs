use std::process::Command;

#[path = "support/rank_n/mod.rs"]
mod cases;

#[test]
fn a_known_application_result_rejects_rigid_mismatch_before_escape() {
    for source in [cases::RIGID_RESULT, cases::RIGID_NOMINAL_RESULT] {
        let errors = psrs_driver::check_source("Main.purs", source)
            .expect_err("a universally quantified callback cannot return its argument as Int");
        assert!(
            errors
                .iter()
                .any(|error| error.code == Some("TypesDoNotUnify")),
            "{errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| error.code == Some("EscapedSkolem")),
            "{errors:?}"
        );
    }
}

#[test]
fn source_rank_n_acceptance_and_rejection() {
    let mut failures = Vec::new();
    if let Err(errors) = psrs_driver::check_program(cases::LINKED) {
        failures.push(format!("linked: {errors:?}"));
    }
    if let Err(errors) = psrs_driver::check_program(cases::aliases::IMPORTED) {
        failures.push(format!("imported aliases: {errors:?}"));
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
    match psrs_driver::compile_program_sources(cases::aliases::IMPORTED) {
        Ok(artifact) => {
            if let Err(error) = expect_execution("imported_aliases", &artifact.wasm) {
                failures.push(error);
            }
        }
        Err(errors) => failures.push(format!("imported aliases: {errors:?}")),
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

#[test]
fn official_operator_alias_executes_and_missing_alias_evidence_is_rejected() {
    let errors = psrs_driver::check_source("missing-alias", cases::aliases::MISSING)
        .expect_err("an alias use still requires its instance");
    assert!(
        errors
            .iter()
            .any(|error| error.code == Some("NoInstanceFound")),
        "{errors:?}"
    );
    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert_ne!(std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(), Ok("1"));
        return;
    }
    let artifact = psrs_driver::compile_program_sources_with_prelude(&[(
        "2803.purs",
        include_str!("../../../tests/upstream/passing/2803.purs"),
    )])
    .expect("the unchanged official operator alias program compiles");
    let path = std::env::temp_dir().join(format!("psrs-2803-{}.wasm", std::process::id()));
    std::fs::write(&path, artifact.wasm).unwrap();
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "Done",
        "{output:?}"
    );
}

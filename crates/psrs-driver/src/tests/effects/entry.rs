use super::super::super::*;
use std::process::Output;

#[test]
fn both_single_source_apis_compile_int_main_with_the_trusted_effect_library() {
    let source = "module Main where\nimport Prelude\nmain = 0\n";
    compile_source("Main.purs", source).expect("compile_source must lower trusted Effect imports");
    compile_source_with_dumps("Main.purs", source)
        .expect("compile_source_with_dumps must lower trusted Effect imports");
}

#[test]
fn an_effect_unit_entry_executes_its_action_once_through_both_source_apis() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = log \"command\"\n";
    let artifact = compile_source("Main.purs", source).expect("inferred Effect Unit main");
    let dumped = compile_source_with_dumps("Main.purs", source)
        .expect("the dump API must preserve Effect lowering context");
    for wasm in [&artifact.wasm, &dumped.artifact.wasm] {
        let Some(output) = run_wasm(wasm) else { return };
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"command\n");
    }
}

#[test]
fn effect_unit_entry_runs_strict_construction_effects_before_its_action() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let ignored = runEffect (log \"construct\") in log \"action\"\n";
    let artifact = compile_source("Main.purs", source).expect("the selected main owns runEffect");
    let Some(output) = run_wasm(&artifact.wasm) else {
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"construct\naction\n");
}

#[test]
fn effect_unit_entry_propagates_a_trap_from_the_action() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain :: Effect Unit\nmain = bind (log \"before\") (\\_ -> bind trap (\\_ -> log \"after\"))\n";
    let artifact = compile_source("Main.purs", source).expect("the effect entry should compile");
    let Some(output) = run_wasm(&artifact.wasm) else {
        return;
    };
    assert!(
        !output.status.success(),
        "the action trap must escape: {output:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("wasm trap:")
            || String::from_utf8_lossy(&output.stderr).contains("wasm backtrace"),
        "the action trap should include a Wasmtime trap marker: {output:?}"
    );
    assert_eq!(
        output.stdout, b"before\n",
        "the trap must suppress later output"
    );
}

#[test]
fn the_effect_context_survives_backend_stage_recompilation() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain :: Effect Unit\nmain = log \"recompiled\"\n";
    let lowered = crate::lower_source_with_prelude_to_core("Main.purs", source).unwrap();
    let core = lowered.core;
    let context = lowered.effect_context;
    let stages = psrs_backend::compile_with_context(
        core,
        context,
        psrs_backend::TargetCapabilities::default(),
    )
    .unwrap();
    let replay = psrs_backend::compile_with_context(
        stages.core.clone(),
        stages.effect_context.clone(),
        psrs_backend::TargetCapabilities::default(),
    )
    .expect("P8 context must be retained with the returned Core");
    let Some(output) = run_wasm(&replay.artifact.wasm) else {
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"recompiled\n");
}

#[test]
fn effect_import_alias_is_expanded_before_suspension_planning() {
    let types = (
        "Types.purs",
        "module Types where\nimport Prelude\ntype ClockTick = Effect Int\n",
    );
    let clock = (
        "Clock.purs",
        "module Clock where\nimport Prelude\nimport Types\nforeign import \"wasi:clocks/monotonic-clock#now\" clock :: ClockTick\n",
    );
    let main = (
        "Main.purs",
        "module Main where\nimport Prelude\nimport WASI.Console\nimport Clock\nmain :: Effect Unit\nmain = bind clock (\\_ -> log \"clock completed\")\n",
    );
    let artifact = compile_program_sources_with_prelude(&[types, clock, main])
        .expect("an imported alias in a WIT signature must be classified from checked types");
    let Some(output) = run_wasm(&artifact.wasm) else {
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"clock completed\n");
}

#[test]
fn effect_suspension_conformance_errors_keep_the_imports_source_origin() {
    let clock = (
        "Clock.purs",
        "module Clock where\nimport Prelude\nforeign import \"wasi:clocks/monotonic-clock#now\" clock :: Effect String\n",
    );
    let main = (
        "Main.purs",
        "module Main where\nimport Prelude\nimport WASI.Console\nimport Clock\nmain = bind clock (\\_ -> log \"unreachable\")\n",
    );
    let errors = compile_program_sources_with_prelude(&[clock, main])
        .expect_err("a suspended host import must still match its WIT result");
    assert!(
        errors.iter().any(|error| {
            error.source == DiagnosticOrigin::Source(0)
                && error.diagnostic.stage == "P8 WIT linking"
                && error
                    .diagnostic
                    .message
                    .contains("source result type incompatible")
        }),
        "{errors:#?}"
    );
}

#[test]
fn class_constrained_wit_imports_are_rejected_with_a_source_diagnostic() {
    let source = "module Main where\nimport Prelude\nforeign import \"wasi:clocks/monotonic-clock#now\" clock :: forall a. Eq a => a\nmain = 0\n";
    let errors = compile_program_sources_with_prelude(&[("Main.purs", source)])
        .expect_err("the canonical WIT ABI cannot carry class dictionaries");
    assert!(
        errors.iter().any(|error| {
            error.source == DiagnosticOrigin::Source(0)
                && error.diagnostic.stage == "P5 typecheck"
                && error
                    .diagnostic
                    .message
                    .contains("class-constrained WIT imports are not supported")
        }),
        "{errors:#?}"
    );
}

#[test]
fn instance_member_references_do_not_bypass_the_run_effect_scope() {
    let source = "module Main where\nimport Prelude\nclass Runner a where\n  run :: a -> Int\ninstance Runner Int where\n  run _ = runEffect (pure 42)\nmain = run 0\n";
    let errors = compile_program_sources_with_prelude(&[("Main.purs", source)])
        .expect_err("instance methods are executable declarations too");
    assert!(
        errors.iter().any(|error| {
            error.source == DiagnosticOrigin::Source(0)
                && error.diagnostic.stage == "P7 entry selection"
                && error
                    .diagnostic
                    .message
                    .contains("may only be referenced from the selected command entry")
        }),
        "{errors:#?}"
    );
}

#[test]
fn the_selected_entry_may_pass_run_effect_to_a_higher_order_helper() {
    let source = "module Main where\nimport Prelude\nrunWith :: (Effect Int -> Int) -> Int\nrunWith runner = runner (pure 42)\nmain = runWith runEffect\n";
    let artifact = compile_source("Main.purs", source)
        .expect("the selected declaration may pass its lexically scoped runner");
    let Some(output) = run_wasm(&artifact.wasm) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn effect_unit_entry_accepts_a_type_synonym_and_cross_module_value() {
    let producer = (
        "Producer.purs",
        "module Producer where\nimport Prelude\nimport WASI.Console\ntype Action = Effect Unit\naction :: Action\naction = log \"linked\"\n",
    );
    let main = (
        "Main.purs",
        "module Main where\nimport Prelude\nimport Producer\nmain = action\n",
    );
    let artifact = compile_program_sources_with_prelude(&[producer, main])
        .expect("an inferred entry may return an imported aliased Effect Unit");
    let Some(output) = run_wasm(&artifact.wasm) else {
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"linked\n");
}

#[test]
fn an_effect_int_entry_remains_invalid() {
    let source = "module Main where\nimport Prelude\nmain :: Effect Int\nmain = pure 42\n";
    let errors = compile_source("Main.purs", source)
        .expect_err("the command entry must return Int or Effect Unit");
    assert!(errors.iter().any(|error| {
        error.stage == "P7 entry selection" && error.message.contains("Effect Unit")
    }));
}

#[test]
fn typechecking_does_not_require_a_unique_command_entry() {
    let left = ("Left.purs", "module Left where\nmain = 0\n");
    let right = ("Right.purs", "module Right where\nmain = 0\n");
    crate::typecheck_program_sources(&[left, right])
        .expect("ordinary typechecking does not select a command entry");
    let errors = compile_program_sources_with_prelude(&[left, right])
        .expect_err("compilation requires one command entry");
    assert!(errors.iter().any(|error| {
        error.diagnostic.stage == "P7 entry selection"
            && error
                .diagnostic
                .message
                .contains("multiple `main` declarations")
    }));
}

#[test]
fn a_main_in_the_main_module_takes_precedence_and_unique_main_is_the_fallback() {
    let other = ("Other.purs", "module Other where\nmain = 1\n");
    let main = ("Main.purs", "module Main where\nmain = 42\n");
    let preferred = compile_program_sources(&[other, main]).unwrap();
    let Some(output) = run_wasm(&preferred.wasm) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));

    let unique = ("Runner.purs", "module Runner where\nmain = 7\n");
    let fallback = compile_program_sources(&[unique]).unwrap();
    let Some(output) = run_wasm(&fallback.wasm) else {
        return;
    };
    assert_eq!(output.status.code(), Some(7));
}

fn run_wasm(wasm: &[u8]) -> Option<Output> {
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        if std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1") {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
        }
        eprintln!("skipping: wasmtime is not installed");
        return None;
    }
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "psrs-effect-entry-{}-{id}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(path);
    Some(output)
}

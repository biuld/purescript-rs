//! Consume the separately owned public library cases through the real pipeline.
use super::*;

pub(super) mod tagged;

fn source(name: &str) -> String {
    let root = crate::standard_library_info().expect("stdlib package").root;
    std::fs::read_to_string(
        root.join("conformance/control/except")
            .join(format!("{name}.purs")),
    )
    .expect("the locked package contains the public Except cases")
}

fn execute(name: &str, trap: bool, stdout: &[u8]) {
    let Some(output) = run_program_with_prelude_wasmtime(&[("Main.purs", &source(name))]) else {
        return;
    };
    assert_eq!(output.stdout, stdout, "{name}: {output:?}");
    if trap {
        assert!(!output.status.success(), "{name}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("wasm trap: wasm `unreachable` instruction executed"),
            "{name}: {output:?}"
        );
    } else {
        assert_eq!(output.status.code(), Some(42), "{name}: {output:?}");
        assert!(output.stderr.is_empty(), "{name}: {output:?}");
    }
}

#[test]
fn complete_public_api_and_reference_run_agree() {
    execute("PublicApi", false, b"");
    execute("ReferenceRun", false, b"");
}

#[test]
fn completion_precedes_recovery_for_raw_and_preconstructed_resume() {
    execute("ExpiredRaw", true, b"");
    execute("ExpiredResume", true, b"");
}

#[test]
fn branch_failure_preserves_outer_template_and_expires_branch_alias() {
    execute("OuterCheckpoint", false, b"");
    execute("ReplayExpired", true, b"");
}

#[test]
fn failure_does_not_reverse_mutation_or_external_output() {
    execute("AbortAndSharing", false, b"");
    execute("ExternalOutput", false, b"before\nrecovery\n");
}

#[test]
fn native_exceptions_remain_outside_the_typed_failure_channel() {
    execute("NativePositions", false, b"");
}

#[test]
fn exception_effect_rejects_a_profile_without_required_native_cleanup() {
    let errors = crate::program::compile_program_sources_with_prelude_target(
        &[("Main.purs", &source("Labels"))],
        psrs_backend::TargetCapabilities {
            exceptions: false,
            ..Default::default()
        },
    )
    .expect_err("Except does not silently remove the runner bridge");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.message.contains("exception-handling")),
        "{errors:?}"
    );
}

#[test]
fn official_error_values_execute_alongside_typed_failure_effects() {
    let source = r#"module Main where
import Prelude
import Effect.Exception (error, message)
main :: Int
main = if message (error "failure") == "failure" then 42 else 0
"#;
    let Some(output) = run_program_with_prelude_wasmtime(&[("Main.purs", source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
}

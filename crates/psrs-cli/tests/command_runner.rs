use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const TYPES: &str = "module Types where\nimport Prim.State (State, RealWorld)\ntype Step s a = { state :: State s, value :: a }\nnewtype Job a = Job (State RealWorld -> Step RealWorld a)\n";
const RUNNER: &str = "module Runner where\nimport Prim.State (State, RealWorld)\nimport Types (Job(..), Step)\nforeign import \"psrs:intrinsic#runWorld\" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a\nrun :: Job Int -> Int\nrun (Job action) = execute action\n";
const MAIN: &str = "module Main where\nimport Types (Job(..))\nmain :: Job Int\nmain = Job (\\state -> { state: state, value: 42 })\n";

struct Package(PathBuf);

impl Package {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "psrs-command-runner-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::create_dir_all(root.join("conformance")).unwrap();
        fs::write(root.join("upstream-lock.json"), "{}").unwrap();
        fs::write(root.join("lib/trusted"), "Prelude\nTypes\nRunner\n").unwrap();
        fs::write(root.join("lib/Prelude.purs"), "module Prelude where\n").unwrap();
        fs::write(root.join("lib/Types.purs"), TYPES).unwrap();
        fs::write(root.join("lib/Runner.purs"), RUNNER).unwrap();
        fs::write(root.join("Main.purs"), MAIN).unwrap();
        let package = Self(root);
        package.configure("Runner", "run");
        package
    }

    fn configure(&self, module: &str, function: &str) {
        let manifest = serde_json::json!({
            "schema_version": 1,
            "name": "psrs-stdlib",
            "source_root": "lib",
            "trusted_modules": "lib/trusted",
            "upstream_lock": "upstream-lock.json",
            "compiler_contract": {
                "primitive_binding_protocol": 1,
                "string_semantics": "unicode-scalar-utf8",
                "int_semantics": "signed-i32",
                "command_runner": { "module": module, "function": function }
            }
        });
        fs::write(
            self.0.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn build(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_psrs"))
            .env("PSRS_STDLIB_ROOT", &self.0)
            .arg("build")
            .arg(self.0.join("Main.purs"))
            .arg("-o")
            .arg(self.0.join("main.wasm"))
            .output()
            .unwrap()
    }

    fn reject(&self, message: &str) {
        let output = self.build();
        assert!(!output.status.success(), "invalid runner accepted");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(message), "{stderr}");
    }

    fn execute(&self, expected: i32) {
        let output = self.build();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new("wasmtime")
            .arg("run")
            .arg(self.0.join("main.wasm"))
            .output();
        match output {
            Ok(output) => assert_eq!(
                output.status.code(),
                Some(expected),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                assert_ne!(
                    std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(),
                    Ok("1"),
                    "mandatory Wasmtime is missing"
                );
                eprintln!("Wasmtime unavailable; execution skipped");
            }
            Err(error) => panic!("Wasmtime failed: {error}"),
        }
    }
}

impl Drop for Package {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn reports_and_diagnosis_retain_the_configured_runner() {
    const CHILD_ROOT: &str = "PSRS_COMMAND_REPORT_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let source = fs::read_to_string(PathBuf::from(root).join("Main.purs")).unwrap();
        let sources = [("Main.purs", source.as_str())];
        let report = psrs_driver::compile_program_sources_with_prelude_report(&sources);
        assert!(report.artifact.is_some(), "{:?}", report.diagnostics);
        assert!(report.dumps.core.unwrap().contains("$command_entry"));
        let report = psrs_driver::compile_program_sources_with_prelude_diagnosis(&sources, true);
        assert!(report.artifact.is_some(), "{:?}", report.diagnostics);
        assert!(report.backend_trace.is_some());
        let frontend = report.frontend_trace.unwrap();
        assert!(frontend.output_core.is_some());
        assert!(
            frontend
                .source_names
                .iter()
                .any(|name| name.ends_with("Runner.purs"))
        );
        return;
    }
    let package = Package::new();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "reports_and_diagnosis_retain_the_configured_runner",
            "--nocapture",
        ])
        .env(CHILD_ROOT, &package.0)
        .env("PSRS_STDLIB_ROOT", &package.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn configured_source_runner_executes_an_unrelated_state_newtype() {
    let package = Package::new();
    fs::write(
        package.0.join("lib/Prelude.purs"),
        "module Prelude where\nforeign import data Effect :: Type -> Type\n",
    )
    .unwrap();
    fs::write(
        package.0.join("Main.purs"),
        MAIN.replace("import Types", "import Prelude (Effect)\nimport Types"),
    )
    .unwrap();
    // Main neither imports Runner nor calls it: configuration must retain it.
    // An unrelated opaque Effect declaration must not activate legacy checks.
    package.execute(42);
    fs::write(
        package.0.join("Main.purs"),
        "module Main where\nmain :: Int\nmain = 7\n",
    )
    .unwrap();
    package.execute(7);
}

#[test]
fn configured_runner_rejects_missing_identity_and_invalid_checked_types() {
    let package = Package::new();
    package.configure("Missing", "run");
    package.reject("absent from the trusted package inventory");
    package.configure("Runner", "missing");
    package.reject("exactly one source declaration");
    package.configure("Runner", "run");
    fs::write(
        package.0.join("lib/Runner.purs"),
        "module Runner where\nimport Types (Job)\nrun :: Job Int -> Boolean\nrun _ = true\n",
    )
    .unwrap();
    package.reject("must return Int");
    fs::write(
        package.0.join("lib/Runner.purs"),
        "module Runner where\nrun :: forall a. a -> Int\nrun _ = 0\n",
    )
    .unwrap();
    package.reject("must be monomorphic");
    fs::write(
        package.0.join("lib/Runner.purs"),
        "module Runner where\nnewtype Other = Other Int\nrun :: Other -> Int\nrun (Other value) = value\n",
    )
    .unwrap();
    package.reject("entry type disagrees");
}

#[test]
fn check_only_does_not_require_an_executable_runner_root() {
    const CHILD_ROOT: &str = "PSRS_COMMAND_CHECK_ONLY_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let source = fs::read_to_string(PathBuf::from(root).join("Main.purs")).unwrap();
        psrs_driver::check_program_types_lenient_with_prelude(&[("Main.purs", &source)]).unwrap();
        let sources = [("Main.purs", source.as_str())];
        for report in [
            psrs_driver::compile_program_sources_with_prelude_report(&sources),
            psrs_driver::compile_program_sources_with_prelude_diagnosis(&sources, true),
        ] {
            assert!(report.artifact.is_none());
            assert!(report.diagnostics.iter().any(|error| {
                error.source == psrs_driver::DiagnosticOrigin::Library
                    && error
                        .diagnostic
                        .message
                        .contains("absent from the trusted package inventory")
            }));
        }
        return;
    }
    let package = Package::new();
    package.configure("Missing", "run");
    // A child process isolates the package cache from other integration cases.
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "check_only_does_not_require_an_executable_runner_root",
            "--nocapture",
        ])
        .env(CHILD_ROOT, &package.0)
        .env("PSRS_STDLIB_ROOT", &package.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    package.reject("absent from the trusted package inventory");
}

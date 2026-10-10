use super::*;

fn lower(
    main: &str,
    library: &str,
    runner: &str,
) -> Result<psrs_core::Module, Vec<ProgramDiagnostic>> {
    crate::program::lower_program_to_core_with_runner(
        &[("Main.purs", main), ("Runner.purs", library)],
        "Runner",
        runner,
    )
}

const LIBRARY: &str = "module Runner where\nnewtype Command = Command Int\nrun :: Command -> Int\nrun (Command value) = value\n";
const MAIN: &str =
    "module Main where\nimport Runner (Command(..))\nmain :: Command\nmain = Command 42\n";

#[test]
fn configured_runner_wraps_an_unrelated_library_type_before_pruning() {
    let core = lower(MAIN, LIBRARY, "run").unwrap();
    let entry = core
        .declarations
        .iter()
        .find(|decl| Some(decl.symbol) == core.entry)
        .unwrap();
    assert_eq!(entry.name, "$command_entry");
    let psrs_core::ExprKind::Application(function, argument) = &entry.value.kind else {
        panic!("ordinary runner application expected")
    };
    assert!(
        matches!(function.kind, psrs_core::ExprKind::Global(symbol) if core.declarations.iter().any(|decl| decl.symbol == symbol && decl.name == "run"))
    );
    assert!(
        matches!(argument.kind, psrs_core::ExprKind::Global(symbol) if core.declarations.iter().any(|decl| decl.symbol == symbol && decl.name == "main"))
    );
    core.verify().unwrap();
    psrs_backend::compile(core).unwrap();
}

#[test]
fn configured_runner_preserves_int_entries_and_rejects_invalid_selection_or_types() {
    let core = lower("module Main where\nmain :: Int\nmain = 7\n", LIBRARY, "run").unwrap();
    assert_eq!(
        core.declarations
            .iter()
            .find(|decl| Some(decl.symbol) == core.entry)
            .unwrap()
            .name,
        "main"
    );
    for (library, runner, message) in [
        (LIBRARY, "missing", "exactly one source declaration"),
        (
            "module Runner where\nnewtype Command = Command Int\nrun :: forall a. a -> Int\nrun _ = 42\n",
            "run",
            "must be monomorphic",
        ),
        (
            "module Runner where\nnewtype Command = Command Int\nrun :: Command -> Boolean\nrun _ = true\n",
            "run",
            "must return Int",
        ),
        (
            "module Runner where\nnewtype Command = Command Int\nnewtype Other = Other Int\nrun :: Other -> Int\nrun (Other value) = value\n",
            "run",
            "disagrees with the configured runner input",
        ),
    ] {
        let errors = lower(MAIN, library, runner).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.message.contains(message)),
            "{errors:?}"
        );
    }
}

#[test]
fn command_normalization_failure_preserves_the_checked_core() {
    let mut core = lower(MAIN, LIBRARY, "run").unwrap();
    let before = core.clone();
    let entry = core
        .declarations
        .iter()
        .find(|decl| decl.name == "main")
        .unwrap()
        .symbol;
    let unknown = psrs_hir::SymbolId::new(entry.module, u32::MAX);
    assert!(psrs_core::command::normalize_entry(&mut core, entry, unknown).is_err());
    assert_eq!(core, before);
}

#[test]
fn configured_runner_executes_a_state_newtype_without_an_explicit_main_runner_call() {
    let library = r#"module Runner where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
newtype Job a = Job (State RealWorld -> Step RealWorld a)
run :: Job Int -> Int
run (Job action) = execute action
"#;
    let main = r#"module Main where
import Runner (Job(..))
main :: Job Int
main = Job (\state -> { state: state, value: 42 })
"#;
    let core = lower(main, library, "run").unwrap();
    if wasmtime_available().is_none() {
        return;
    }
    let artifact = psrs_backend::compile(core).unwrap();
    let path = std::env::temp_dir().join(format!("psrs-command-state-{}.wasm", std::process::id()));
    std::fs::write(&path, artifact.wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        output.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

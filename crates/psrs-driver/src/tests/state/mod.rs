//! State contracts compose with ordinary source newtypes and nominal regions.
use super::*;

mod cc;
mod command;
mod host;
mod runners;
mod runtime_cc;
mod storage_protocol;

fn state_flow_source(body: &str) -> String {
    format!(
        r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = {{ state :: State s, value :: a }}
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
foreign import "psrs:runtime-storage#array_read" read :: forall s a. Array a -> Int -> State s -> Step s a
action :: State RealWorld -> Step RealWorld Int
action state0 = {body}
main :: Int
main = execute action
"#
    )
}

#[test]
fn source_state_calls_produce_core_dependencies_before_projection() {
    let source = state_flow_source(
        r#"case read [40] 0 state0 of
  { state: state1, value: first } -> case read [first] 0 state1 of
    { state: state2, value: second } -> { state: state2, value: second }"#,
    );
    let core = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let flows = psrs_core::state::flow::check(&core).unwrap();
    let action = flows
        .iter()
        .find(|flow| flow.operations.len() == 2)
        .expect("two actual runtime-bound source invocations");
    action.verify(&core).unwrap();
    assert_eq!(action.operations[1].input, action.operations[0].output);
}

#[test]
fn source_state_flow_rejects_stale_returns_and_replayed_predecessors() {
    for (body, message) in [
        (
            r#"case read [40] 0 state0 of
  { state: state1, value: first } -> { state: state0, value: first }"#,
            "state return discards an executed operation",
        ),
        (
            r#"case read [40] 0 state0 of
  { state: state1, value: first } -> read [first] 0 state0"#,
            "state call consumes a stale dependency",
        ),
    ] {
        let source = state_flow_source(body);
        check_program(&[("Main.purs", &source)]).unwrap();
        let errors = lower_program_to_core(&[("Main.purs", &source)]).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P7 Core verification"
                    && error.diagnostic.message == message),
            "{errors:?}"
        );
    }
}

#[test]
fn source_state_branch_joins_preserve_the_selected_dependency() {
    let source = state_flow_source(
        r#"case (if true then read [40] 0 state0 else read [2] 0 state0) of
  { state: state1, value: first } -> read [first] 0 state1"#,
    );
    let core = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let flows = psrs_core::state::flow::check(&core).unwrap();
    let action = flows
        .iter()
        .find(|flow| flow.operations.len() == 3)
        .unwrap();
    assert_eq!(action.graph.blocks.len(), 4);
    action.verify(&core).unwrap();
}

#[test]
fn ordinary_library_newtype_bind_retains_checked_core_state_flow() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
newtype Task a = Task (State RealWorld -> Step RealWorld a)
pureTask :: forall a. a -> Task a
pureTask value = Task (\state -> { state: state, value: value })
bindTask :: forall a b. Task a -> (a -> Task b) -> Task b
bindTask (Task first) next = Task (\state0 ->
  case first state0 of
    { state: state1, value: value } ->
      case next value of
        Task second -> second state1)
main :: Int
main = case bindTask (pureTask 40) (\value -> pureTask (intAdd value 2)) of
  Task action -> execute action
"#;
    let core = lower_program_to_core(&[("Main.purs", source)]).unwrap();
    let flows = psrs_core::state::flow::check(&core).unwrap();
    assert!(flows.iter().any(|flow| flow.operations.is_empty()));
    assert!(flows.iter().any(|flow| flow.operations.len() == 2));
}

#[test]
fn state_newtype_combinators_are_ordinary_library_definitions() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
newtype Task a = Task (State RealWorld -> Step RealWorld a)
type role Task representational
pureTask :: forall a. a -> Task a
pureTask value = Task (\state -> { state: state, value: value })
bindTask :: forall a b. Task a -> (a -> Task b) -> Task b
bindTask (Task first) next = Task (\state0 ->
  case first state0 of
    { state: state1, value: value } ->
      case next value of
        Task second -> second state1)
main :: Int
main = 0
"#;
    check_program(&[("Main.purs", source)])
        .unwrap_or_else(|errors| panic!("ordinary state combinators: {errors:?}"));
}

#[test]
fn the_state_region_cannot_be_safely_coerced() {
    let source = r#"module Main where
import Prim.State (State)
import Safe.Coerce (coerce)
newtype RegionA = RegionA Int
newtype RegionB = RegionB Int
mix :: State RegionA -> State RegionB
mix = coerce
main :: Int
main = 0
"#;
    let errors = check_program(&[("Main.purs", source)]).expect_err("nominal state regions");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
        "{errors:?}"
    );
}

#[test]
fn unsafe_state_operations_require_an_explicit_binding() {
    let unbound = "module Main where\nmain = runWorld (\\state -> { state: state, value: 42 })\n";
    let errors = check_program(&[("Main.purs", unbound)]).expect_err("no implicit unsafe runner");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage == "P3 resolve"),
        "{errors:?}"
    );
    let bound = r#"module Main where
import Prim.State (State, RealWorld)
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> { state :: State RealWorld, value :: a }) -> a
main :: Int
main = execute (\state -> { state: state, value: 42 })
"#;
    check_program(&[("Main.purs", bound)])
        .unwrap_or_else(|errors| panic!("explicit state runner scheme: {errors:?}"));
}

#[test]
fn state_binding_validation_rejects_invalid_unused_runner_schemes() {
    for contract in [
        "forall s a. (State s -> { state :: State s, value :: a }) -> a",
        "forall a. (State RealWorld -> { state :: State RealWorld, value :: a }) -> State RealWorld",
        "forall a. (State RealWorld -> { state :: State RealWorld, value :: a, extra :: Int }) -> a",
    ] {
        let source = format!(
            "module Main where\nimport Prim.State (State, RealWorld)\nforeign import \"psrs:intrinsic#runWorld\" execute :: {contract}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("invalid unused state binding");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking"
                    && !error
                        .diagnostic
                        .message
                        .contains("no foreign-function implementation")),
            "{errors:?}"
        );
    }
}

fn runtime_source(export: &str, scheme: &str) -> String {
    format!(
        "module Main where\nimport Prim.State (State, RealWorld)\ntype Step s a = {{ state :: State s, value :: a }}\nforeign import \"psrs:runtime-storage#{export}\" operation :: {scheme}\nmain :: Int\nmain = 0\n"
    )
}

#[test]
fn storage_uses_ordinary_runtime_binding_identity() {
    let source = runtime_source(
        "array_read",
        "forall s a. Array a -> Int -> State s -> Step s a",
    );
    let modules = resolve_program_sources(&[("Main.purs", &source)]).unwrap();
    let external = modules[0]
        .externals
        .iter()
        .find(|external| external.name == "operation")
        .unwrap();
    assert_eq!(
        external.kind,
        psrs_hir::ExternalKind::Runtime {
            module: "psrs:runtime-storage".into(),
            function: "array_read".into(),
        }
    );
}

#[test]
fn checked_unused_runtime_storage_schemes_compile_without_imports() {
    for (export, scheme) in [
        (
            "array_read",
            "forall s a. Array a -> Int -> State s -> Step s a",
        ),
        (
            "array_fill",
            "forall s a. Int -> a -> State s -> Step s (Array a)",
        ),
        (
            "array_write",
            "forall s a. Array a -> Int -> a -> State s -> Step s Unit",
        ),
        ("trap", "forall s a. State s -> Step s a"),
    ] {
        let source = runtime_source(export, scheme);
        check_program(&[("Main.purs", &source)])
            .unwrap_or_else(|errors| panic!("{export}: {errors:?}"));
        compile_program_sources(&[("Main.purs", &source)])
            .unwrap_or_else(|errors| panic!("{export}: {errors:?}"));
    }
}

#[test]
fn runtime_binding_contracts_reject_invalid_unused_declarations() {
    for (export, scheme) in [
        (
            "array_read",
            "forall s a. Array a -> Boolean -> State s -> Step s a",
        ),
        (
            "array_read",
            "forall s a. Array a -> Int -> State s -> Step s Int",
        ),
        (
            "array_read",
            "forall s t a. Array a -> Int -> State s -> Step t a",
        ),
        (
            "array_read",
            "forall s a. Array a -> Int -> State s -> { state :: State s, value :: a, extra :: Int }",
        ),
        (
            "array_fill",
            "forall s a. Int -> a -> State s -> Step s Int",
        ),
        (
            "array_fill",
            "forall s a. Int -> a -> State s -> Step s (Array Int)",
        ),
        (
            "array_write",
            "forall s a b. Array a -> Int -> b -> State s -> Step s Unit",
        ),
        (
            "array_write",
            "forall s a. Array a -> Int -> a -> State s -> Step s Int",
        ),
        ("trap", "Int -> Int"),
    ] {
        let source = runtime_source(export, scheme);
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("invalid unused runtime binding");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 runtime linking"),
            "{export}: {errors:?}"
        );
    }
}

#[test]
fn unknown_runtime_exports_and_retired_storage_intrinsics_are_rejected() {
    let source = runtime_source("missing", "forall s a. State s -> Step s a");
    let errors =
        compile_program_sources(&[("Main.purs", &source)]).expect_err("unknown runtime export");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage == "P8 runtime linking"
                && error.diagnostic.message.contains("no export")),
        "{errors:?}"
    );
    let source = runtime_source(
        "array_read",
        "forall s a. Array a -> Int -> State s -> Step s a",
    )
    .replace(
        "psrs:runtime-storage#array_read",
        "psrs:intrinsic#stateArrayRead",
    );
    let errors = check_program(&[("Main.purs", &source)]).expect_err("retired intrinsic");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage == "P3 resolve"),
        "{errors:?}"
    );
}

#[test]
fn state_accepts_the_official_st_region_kind() {
    let source = r#"module Main where
import Prim.State (State)
foreign import data Region :: Type
foreign import data Global :: Region
type Step s a = { state :: State s, value :: a }
foreign import "psrs:runtime-storage#array_read" readGlobal :: forall a. Array a -> Int -> State Global -> Step Global a
foreign import "psrs:intrinsic#runRegion" runLocal :: forall a. (forall (s :: Region). State s -> Step s a) -> a
newtype Action r a = Action (State r -> { state :: State r, value :: a })
type role Action nominal representational
global :: Action Global Int
global = Action (\state -> { state: state, value: 42 })
main :: Int
main = 0
"#;
    check_program(&[("Main.purs", source)])
        .unwrap_or_else(|errors| panic!("official ST region kind: {errors:?}"));
    compile_program_sources(&[("Main.purs", source)])
        .unwrap_or_else(|errors| panic!("official ST region kind compilation: {errors:?}"));
}

#[test]
fn callable_newtype_templates_survive_higher_kinded_storage() {
    let source = r#"module Main where
newtype Reader r a = Reader (r -> a)
newtype Runner = Runner (forall f a. (f a -> Int) -> f a -> Int)
runner :: Runner
runner = Runner (\consume value -> consume value)
run :: Reader Int Int -> Int
run (Reader action) = action 2
main :: Int
main = case runner of
  Runner invoke -> invoke run (Reader (\value -> intAdd value 40))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn structured_callable_newtype_domains_and_results_survive_storage() {
    let source = r#"module Main where
newtype Batch r a = Batch (Array r -> { tag :: Int, value :: a })
newtype Runner = Runner (forall f a. (f a -> Int) -> f a -> Int)
runner :: Runner
runner = Runner (\consume value -> consume value)
run :: Batch Int Int -> Int
run (Batch action) = let result = action [2] in intAdd result.tag result.value
main :: Int
main = case runner of
  Runner invoke -> invoke run (Batch (\values -> { tag: arrayIndex values 0, value: 40 }))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn higher_kinded_adt_field_preserves_callable_newtype() {
    let source = r#"module Main where
data Holder f a = Holder (f a)
newtype Reader r a = Reader (r -> a)
hold :: forall f a. f a -> Holder f a
hold value = Holder value
recover :: forall f a. Holder f a -> f a
recover (Holder value) = value
run :: Reader Int Int -> Int
run (Reader action) = action 2
main :: Int
main = run (recover (hold (Reader (\value -> intAdd value 40))))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn higher_kinded_adt_field_preserves_structured_callable_newtype() {
    let source = r#"module Main where
data Holder f a = Holder (f a)
newtype Batch r a = Batch (Array r -> { tag :: Int, value :: a })
hold :: forall f a. f a -> Holder f a
hold value = Holder value
recover :: forall f a. Holder f a -> f a
recover (Holder value) = value
run :: Batch Int Int -> Int
run (Batch action) = let result = action [2] in intAdd result.tag result.value
main :: Int
main = run (recover (hold (Batch (\values -> { tag: arrayIndex values 0, value: 40 }))))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ordinary_state_callbacks_lower_to_checked_cc_instruction_dependencies() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
apply :: (State RealWorld -> Step RealWorld Int) -> State RealWorld -> Step RealWorld Int
apply action state = action state
ignore :: (State RealWorld -> Step RealWorld Int) -> Int
ignore action = 42
main :: Int
main = ignore (apply (\state -> { state: state, value: 40 }))
"#;
    let core = lower_program_to_core(&[("Main.purs", source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let flows = psrs_backend::cc::state::check(&input.cc).unwrap();
    let apply = flows
        .iter()
        .find(|flow| {
            flow.operations.len() == 1
                && matches!(
                    flow.operations[0].assignment.kind,
                    psrs_backend::cc::AssignmentKind::IndirectCall { .. }
                )
        })
        .expect("a source callback invocation survives as an actual CC indirect call");
    assert_eq!(apply.operations.len(), 1);
    assert!(matches!(
        apply.operations[0].assignment.kind,
        psrs_backend::cc::AssignmentKind::IndirectCall { .. }
    ));
    apply.verify(&input.cc).unwrap();
}

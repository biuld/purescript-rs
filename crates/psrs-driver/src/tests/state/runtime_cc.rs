use super::*;

#[test]
fn a_partially_applied_runtime_binding_preserves_its_source_scheme() {
    let source = state_flow_source("let reader = read [42] 0 in reader state0");
    let core = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let binding = &input.externals.runtime[0];
    let flows = psrs_backend::cc::state::check(&input.cc).unwrap();
    let operation = flows.iter().flat_map(|flow| &flow.operations).find(|operation|
        matches!(operation.assignment.kind, psrs_backend::cc::AssignmentKind::DirectCall { function, .. } if function == binding.symbol)).unwrap();
    assert_ne!(operation.input, operation.output);
    for flow in &flows {
        flow.verify(&input.cc).unwrap();
    }
}

#[test]
fn runtime_fill_write_read_preserve_the_cc_successor_chain() {
    let source = state_flow_source(
        r#"case fill 2 40 state0 of
  { state: state1, value: items } -> case write items 0 42 state1 of
    { state: state2, value: ignored } -> read items 0 state2"#,
    ).replace("action ::", r#"foreign import "psrs:runtime-storage#array_fill" fill :: forall s a. Int -> a -> State s -> Step s (Array a)
foreign import "psrs:runtime-storage#array_write" write :: forall s a. Array a -> Int -> a -> State s -> Step s Unit
action ::"#);
    let core = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    assert_eq!(input.externals.runtime.len(), 3);
    let flows = psrs_backend::cc::state::check(&input.cc).unwrap();
    let flow = flows
        .iter()
        .find(|flow| flow.operations.len() == 3)
        .unwrap();
    flow.verify(&input.cc).unwrap();
    for pair in flow.operations.windows(2) {
        assert_eq!(pair[1].input, pair[0].output);
    }
    let targets = flow
        .operations
        .iter()
        .map(|operation| match operation.assignment.kind {
            psrs_backend::cc::AssignmentKind::DirectCall { function, .. } => input
                .externals
                .runtime
                .iter()
                .find(|binding| binding.symbol == function)
                .unwrap()
                .function
                .as_str(),
            _ => panic!("runtime operations must remain ordinary direct calls"),
        })
        .collect::<Vec<_>>();
    assert_eq!(targets, ["array_fill", "array_write", "array_read"]);
}

#[test]
fn runtime_source_calls_publish_checked_cc_dependencies() {
    let source = state_flow_source(
        r#"case read [40] 0 state0 of
  { state: state1, value: first } -> case read [first] 0 state1 of
    { state: state2, value: second } -> { state: state2, value: second }"#,
    );
    let core = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    assert_eq!(input.externals.runtime.len(), 1);
    let binding = &input.externals.runtime[0];
    assert_eq!(binding.module, "psrs:runtime-storage");
    assert_eq!(binding.function, "array_read");
    assert!(binding.type_id.is_some());
    let external = input
        .cc
        .externals
        .iter()
        .find(|external| external.symbol == binding.symbol)
        .unwrap();
    let signature = external.signature.as_ref().unwrap();
    let projection =
        psrs_backend::cc::state::StateCallProjection::checked(signature, &input.cc.representations)
            .unwrap()
            .unwrap();
    assert_eq!(projection.state_parameter, 2);
    assert_eq!(
        projection.payload,
        psrs_backend::cc::ValueShape::Reference(psrs_backend::cc::Reference {
            nullable: false,
            heap: psrs_backend::cc::RefShape::Erased,
        })
    );
    let flows = psrs_backend::cc::state::check(&input.cc).unwrap();
    let flow = flows
        .iter()
        .find(|flow| flow.operations.len() == 2)
        .unwrap();
    flow.verify(&input.cc).unwrap();
    assert_eq!(flow.operations[1].input, flow.operations[0].output);
    assert!(flow.operations.iter().all(|operation| matches!(operation.assignment.kind,
        psrs_backend::cc::AssignmentKind::DirectCall { function, .. } if function == binding.symbol)));
    let (module, _) = psrs_backend::mir::lower_module_with_bindings(
        input.cc.clone(),
        input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .unwrap();
    psrs_backend::mir::verify_module(&module).unwrap();
    assert_eq!(module.imports.len(), 1);
    assert_eq!(
        module.imports[0].runtime.as_ref().unwrap().function,
        "array_read"
    );
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.state.is_some())
    );
}

#[test]
fn source_storage_projection_preserves_an_ignored_write_through_optimization() {
    let source = storage_alias_source();
    let target = psrs_backend::TargetCapabilities {
        wasi_cli: false,
        ..psrs_backend::TargetCapabilities::default()
    };
    let core = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let (module, mut wasi) =
        psrs_backend::mir::lower_module_with_bindings(input.cc, input.externals, target).unwrap();
    let module = psrs_backend::mir::opt::optimize(module, target).unwrap();
    assert_eq!(
        module
            .imports
            .iter()
            .filter(|import| import.runtime.is_some())
            .count(),
        3
    );
    let write = module
        .imports
        .iter()
        .find(|import| {
            import
                .runtime
                .as_ref()
                .is_some_and(|provider| provider.function == "array_write")
        })
        .unwrap();
    assert!(module.functions.iter().flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions).any(|instruction|
            matches!(instruction, psrs_backend::mir::Instruction::CallVoid { function, .. } if *function == write.symbol)));
    let wasm =
        psrs_backend::wasm::lower_module_with_capabilities(&module, &mut wasi, target).unwrap();
    psrs_backend::wasm::encode_module(&wasm).unwrap();
}

fn storage_alias_source() -> String {
    state_flow_source(r#"case fill 2 40 state0 of
  { state: state1, value: items } -> let alias = items in
    case write items 0 42 state1 of
      { state: state2, value: ignored } -> read alias 0 state2"#)
        .replace("action ::", r#"foreign import "psrs:runtime-storage#array_fill" fill :: forall s a. Int -> a -> State s -> Step s (Array a)
foreign import "psrs:runtime-storage#array_write" write :: forall s a. Array a -> Int -> a -> State s -> Step s Unit
action ::"#)
}

#[test]
fn source_storage_executes_through_the_default_cli_and_preserves_alias_writes() {
    for expected in [42, 40] {
        let source = storage_alias_source()
            .replace(
                "main ::",
                r#"foreign import "psrs:intrinsic#intEq" eq :: Int -> Int -> Boolean
main ::"#,
            )
            .replace(
                "main = execute action",
                &format!("main = if eq (execute action) {expected} then 0 else 1"),
            );
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            assert!(
                std::env::var_os("PSRS_REQUIRE_WASMTIME").is_none(),
                "Wasmtime is required"
            );
            return;
        };
        assert_eq!(
            output.status.code(),
            Some(i32::from(expected != 42)),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn ordinary_newtype_bind_executes_runtime_storage_without_an_effect_dispatcher() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
foreign import "psrs:runtime-storage#array_fill" fill :: forall s a. Int -> a -> State s -> Step s (Array a)
foreign import "psrs:runtime-storage#array_write" write :: forall s a. Array a -> Int -> a -> State s -> Step s Unit
foreign import "psrs:runtime-storage#array_read" read :: forall s a. Array a -> Int -> State s -> Step s a
newtype Task a = Task (State RealWorld -> Step RealWorld a)
bindTask :: forall a b. Task a -> (a -> Task b) -> Task b
bindTask (Task first) next = Task (\state0 ->
  case first state0 of
    { state: state1, value: value } -> case next value of
      Task second -> second state1)
fillTask :: forall a. Int -> a -> Task (Array a)
fillTask length value = Task (\state -> fill length value state)
writeTask :: forall a. Array a -> Int -> a -> Task Unit
writeTask items index value = Task (\state -> write items index value state)
readTask :: forall a. Array a -> Int -> Task a
readTask items index = Task (\state -> read items index state)
runTask :: forall a. Task a -> a
runTask (Task action) = execute action
action :: Task Int
action = bindTask (fillTask 2 40) (\items ->
  let alias = items in bindTask (writeTask items 0 42) (\ignored -> readTask alias 0))
main :: Int
main = runTask action
"#;
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", source)]) else {
        assert!(
            std::env::var_os("PSRS_REQUIRE_WASMTIME").is_none(),
            "Wasmtime is required"
        );
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
fn runtime_nonreturning_calls_trap_and_only_terminate_the_selected_branch() {
    for (body, expected) in [
        ("abort state0", None),
        (
            "case read [0] 0 state0 of { state: state1, value: first } -> if eq first 0 then abort state1 else { state: state1, value: 42 }",
            None,
        ),
        (
            "case read [1] 0 state0 of { state: state1, value: first } -> if eq first 0 then abort state1 else { state: state1, value: 42 }",
            Some(42),
        ),
        (
            "case abort state0 of { state: state1, value: ignored } -> read [42] 0 state1",
            None,
        ),
        (
            "if true then abort state0 else { state: state0, value: 42 }",
            None,
        ),
        (
            "if false then abort state0 else { state: state0, value: 42 }",
            Some(42),
        ),
    ] {
        let source = state_flow_source(body).replace(
            "action ::",
            r#"foreign import "psrs:runtime-storage#trap" abort :: forall s a. State s -> Step s a
foreign import "psrs:intrinsic#intEq" eq :: Int -> Int -> Boolean
action ::"#,
        );
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            assert!(
                std::env::var_os("PSRS_REQUIRE_WASMTIME").is_none(),
                "Wasmtime is required"
            );
            return;
        };
        if let Some(expected) = expected {
            assert_eq!(
                output.status.code(),
                Some(expected),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        } else {
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("unreachable"),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn gc_runtime_and_canonical_allocator_share_the_application_memory() {
    for abort in [false, true] {
        let body = format!(
            "case arguments state0 of {{ state: state1, value: items }} -> if {abort} then abort state1 else read [42] 0 state1"
        );
        let source = state_flow_source(&body).replace("action ::", r#"foreign import "wasi:cli/environment#get-arguments" arguments :: State RealWorld -> Step RealWorld (Array String)
foreign import "psrs:runtime-storage#trap" abort :: forall s a. State s -> Step s a
action ::"#);
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        if abort {
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("wasm `unreachable` instruction executed"),
                "{output:?}"
            );
        } else {
            assert_eq!(output.status.code(), Some(42), "{output:?}");
        }
    }
}

#[test]
fn source_constructor_choices_execute_only_the_selected_state_action() {
    for (choice, expected) in [("First", 42), ("Second", 43), ("Last", 44)] {
        let body = format!("choose {choice} state0");
        let source = state_flow_source(&body).replace(
            "action ::",
            r#"data Choice = First | Second | Last
choose :: Choice -> State RealWorld -> Step RealWorld Int
choose choice state0 = case choice of
  First -> read [42] 0 state0
  Second -> read [43] 0 state0
  Last -> read [44] 0 state0
action ::"#,
        );
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert_eq!(output.status.code(), Some(expected), "{choice}: {output:?}");
    }
}

#[test]
fn constructor_choice_cannot_replay_the_predecessor_after_its_state_operation() {
    let source = state_flow_source(
        r#"case choose First state0 of
  { state: state1, value: item } -> read [item] 0 state0"#,
    )
    .replace(
        "action ::",
        r#"data Choice = First | Second | Last
choose :: Choice -> State RealWorld -> Step RealWorld Int
choose choice state0 = case choice of
  First -> read [42] 0 state0
  Second -> read [43] 0 state0
  Last -> read [44] 0 state0
action ::"#,
    );
    let errors = lower_program_to_core(&[("Main.purs", &source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage == "P7 Core verification"
                && error.diagnostic.message.contains("stale dependency")),
        "{errors:?}"
    );
}

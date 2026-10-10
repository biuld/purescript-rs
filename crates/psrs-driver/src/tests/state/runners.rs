use super::*;

fn world(body: &str) -> String {
    format!(
        r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = {{ state :: State s, value :: a }}
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
main :: Int
main = {body}
"#
    )
}

fn compile(source: &str) -> (psrs_backend::mir::Module, psrs_backend::abi::WasiRegistry) {
    let core = lower_program_to_core(&[("Main.purs", source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let target = psrs_backend::TargetCapabilities::default();
    let (mir, wasi) =
        psrs_backend::mir::lower_module_with_bindings(input.cc, input.externals, target).unwrap();
    let mir = psrs_backend::mir::opt::optimize(mir, target).unwrap();
    (mir, wasi)
}

#[test]
fn source_world_runner_projects_a_closed_invocation() {
    let source = world("execute (\\state -> { state: state, value: 42 })");
    let (mir, mut wasi) = compile(&source);
    assert!(mir.functions.iter().any(|function| {
        function
            .state
            .as_ref()
            .is_some_and(|flow| flow.executions().len() == 1)
    }));
    let wasm = psrs_backend::wasm::lower_module(&mir, &mut wasi).unwrap();
    psrs_backend::wasm::encode_module(&wasm).unwrap();
}

fn execute(source: &str, expected: i32) {
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", source)]) else {
        assert!(
            std::env::var_os("PSRS_REQUIRE_WASMTIME").is_none(),
            "Wasmtime is required"
        );
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn source_world_runner_executes_the_source_selected_entry() {
    execute(
        &world("execute (\\state -> { state: state, value: 42 })"),
        42,
    );
}

#[test]
fn abstract_constructor_transport_preserves_state_and_converts_only_the_payload() {
    execute(
        r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
newtype Job a = Job (State RealWorld -> Step RealWorld a)
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
preserve :: forall f a. f a -> f a
preserve value = value
unwrap :: forall a. Job a -> State RealWorld -> Step RealWorld a
unwrap (Job action) = action
main :: Int
main = execute (unwrap (preserve (Job (\state -> { state: state, value: 42 }))))
"#,
        42,
    );
}

#[test]
fn source_region_runner_executes_a_rank_n_body() {
    let source = r#"module Main where
import Prim.State (State)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runRegion" scoped :: forall a. (forall s. State s -> Step s a) -> a
main :: Int
main = scoped (\state -> { state: state, value: 43 })
"#;
    execute(source, 43);
}

#[test]
fn nested_source_runners_preserve_both_invocation_boundaries() {
    let source = world(
        "execute (\\outer -> { state: outer, value: execute (\\inner -> { state: inner, value: 44 }) })",
    );
    let (mir, _) = compile(&source);
    assert_eq!(
        mir.functions
            .iter()
            .filter_map(|function| function.state.as_ref())
            .map(|flow| flow.executions().len())
            .sum::<usize>(),
        2
    );
    execute(&source, 44);
}

#[test]
fn repeated_source_runners_retain_two_dynamic_calls() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
foreign import "psrs:intrinsic#intAdd" add :: Int -> Int -> Int
action :: State RealWorld -> Step RealWorld Int
action state = { state: state, value: 41 }
main :: Int
main = add (execute action) (execute action)
"#;
    let (mir, _) = compile(source);
    let runners = mir
        .functions
        .iter()
        .filter(|function| {
            function
                .state
                .as_ref()
                .is_some_and(|flow| !flow.executions().is_empty())
        })
        .map(|function| function.symbol)
        .collect::<Vec<_>>();
    let entry = mir
        .functions
        .iter()
        .find(|function| Some(function.symbol) == mir.entry)
        .unwrap();
    assert_eq!(runners.len(), 2);
    let closures = entry
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            psrs_backend::mir::Instruction::ClosureNew {
                destination,
                function,
                ..
            } if runners.contains(function) => Some(*destination),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        entry
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| matches!(instruction,
            psrs_backend::mir::Instruction::ClosureCall { function, .. }
                if closures.contains(function)))
            .count(),
        2
    );
    execute(source, 82);
}

#[test]
fn runner_projection_rejects_removed_calls_and_changed_action_operands() {
    let source = world("execute (\\state -> { state: state, value: 42 })");
    let (mir, _) = compile(&source);
    let index = mir
        .functions
        .iter()
        .position(|function| {
            function
                .state
                .as_ref()
                .is_some_and(|flow| !flow.executions().is_empty())
        })
        .unwrap();
    let mut missing = mir.clone();
    missing.functions[index].blocks[0]
        .instructions
        .retain(|instruction| {
            !matches!(
                instruction,
                psrs_backend::mir::Instruction::ClosureCall { .. }
            )
        });
    assert!(
        psrs_backend::mir::verify_module(&missing)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("loses an invocation"))
    );
    let mut changed = mir;
    let function = &mut changed.functions[index];
    let replacement = function.result;
    if let Some(psrs_backend::mir::Instruction::ClosureCall { function, .. }) = function.blocks[0]
        .instructions
        .iter_mut()
        .find(|instruction| {
            matches!(
                instruction,
                psrs_backend::mir::Instruction::ClosureCall { .. }
            )
        })
    {
        *function = replacement;
    }
    assert!(
        psrs_backend::mir::verify_module(&changed)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("invocation producer"))
    );
}

#[test]
fn ordinary_newtype_pure_and_bind_execute_through_the_library_runner() {
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
runTask :: forall a. Task a -> a
runTask (Task action) = execute action
main :: Int
main = runTask (bindTask (pureTask 40) (\value -> pureTask (intAdd value 6)))
"#;
    execute(source, 46);
}

#[test]
fn source_runner_propagates_a_trap_from_the_invoked_body() {
    let source = world("execute (\\state -> { state: state, value: quotient 1 0 })").replace(
        "main :: Int",
        "foreign import \"psrs:intrinsic#intQuot\" quotient :: Int -> Int -> Int\nmain :: Int",
    );
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
        return;
    };
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("integer divide by zero"), "{stderr}");
}

#[test]
fn a_callable_step_payload_keeps_its_source_operand_during_optimization() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
produce :: State RealWorld -> Step RealWorld (State RealWorld -> Step RealWorld Int)
produce state = { state: state, value: \next -> { state: next, value: 47 } }
main :: Int
main = execute (\initial -> case produce initial of
  { state: next, value: callback } -> callback next)
"#;
    execute(source, 47);
}

#[test]
fn a_state_callable_crosses_a_bare_polymorphic_payload() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
data Holder a = Holder a
unwrap :: forall a. Holder a -> a
unwrap (Holder value) = value
action :: State RealWorld -> Step RealWorld Int
action state = { state: state, value: 42 }
main :: Int
main = execute (unwrap (Holder action))
"#;
    execute(source, 42);
}

#[test]
fn a_state_callable_curries_ordinary_arguments_before_its_dependency() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
foreign import "psrs:intrinsic#intAdd" add :: Int -> Int -> Int
data Holder a = Holder a
unwrap :: forall a. Holder a -> a
unwrap (Holder value) = value
action :: Int -> Int -> State RealWorld -> Step RealWorld Int
action left right state = { state: state, value: add left right }
main :: Int
main = execute ((unwrap (Holder action)) 20 23)
"#;
    execute(source, 43);
}

#[test]
fn state_callables_cross_polymorphic_array_payloads() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
foreign import "psrs:intrinsic#arrayIndex" index :: forall a. Array a -> Int -> a
data Holder a = Holder a
unwrap :: forall a. Holder a -> a
unwrap (Holder value) = value
action :: State RealWorld -> Step RealWorld Int
action state = { state: state, value: 44 }
main :: Int
main = execute (index (unwrap (Holder [action])) 0)
"#;
    execute(source, 44);
}

#[test]
fn stored_state_callables_keep_fresh_region_checking() {
    let source = r#"module Main where
import Prim.State (State)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runRegion" scoped :: forall a. (forall s. State s -> Step s a) -> a
data Holder a = Holder a
unwrap :: forall a. Holder a -> a
unwrap (Holder value) = value
action :: forall s. State s -> Step s Int
action state = { state: state, value: 45 }
main :: Int
main = scoped (\state -> (unwrap (Holder action)) state)
"#;
    execute(source, 45);
    let wrong = source
        .replace(
            "import Prim.State (State)",
            "import Prim.State (State, RealWorld)",
        )
        .replace(
            "action :: forall s. State s -> Step s Int",
            "action :: State RealWorld -> Step RealWorld Int",
        );
    let errors = lower_program_to_core(&[("Main.purs", wrong.as_str())]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage == "P5 typecheck"),
        "{errors:?}"
    );
}

#[test]
fn erased_state_callables_preserve_a_callable_step_payload() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
foreign import "psrs:intrinsic#intAdd" add :: Int -> Int -> Int
data Holder a = Holder a
unwrap :: forall a. Holder a -> a
unwrap (Holder value) = value
action :: State RealWorld -> Step RealWorld (Int -> Int)
action state = { state: state, value: \value -> add value 1 }
main :: Int
main = (execute (unwrap (Holder action))) 45
"#;
    execute(source, 46);
}

#[test]
fn polymorphic_array_transport_preserves_mutable_storage_aliases() {
    let source = r#"module Main where
foreign import "psrs:intrinsic#arrayIndex" index :: forall a. Array a -> Int -> a
foreign import "psrs:intrinsic#arrayWrite" write :: forall a. Array a -> Int -> a -> Array a
data Holder a = Holder a
unwrap :: forall a. Holder a -> a
unwrap (Holder value) = value
main :: Int
main = let original = [10, 20]
           alias = unwrap (Holder original)
           written = write original 0 99
           ignored = write written 1 21
       in index alias 0
"#;
    execute(source, 99);
}

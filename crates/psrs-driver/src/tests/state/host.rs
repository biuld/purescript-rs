//! Source newtypes and canonical host adapters retain one dependency chain.
use super::*;

fn source(declarations: &str, action: &str) -> String {
    format!(
        r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = {{ state :: State s, value :: a }}
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
newtype Task a = Task (State RealWorld -> Step RealWorld a)
{declarations}
action :: Task Int
action = Task (\state0 -> {action})
main :: Int
main = case action of
  Task run -> execute run
"#
    )
}

#[test]
fn source_newtype_host_resources_and_canonical_scalars_execute() {
    for (declarations, action) in [
        (
            r#"foreign import data Stream :: Type
foreign import "wasi:cli/stdout#get-stdout" stdout :: State RealWorld -> Step RealWorld Stream
foreign import "wasi:io/streams#[resource-drop]output-stream" release :: Stream -> State RealWorld -> Step RealWorld Unit"#,
            r#"case stdout state0 of
    { state: state1, value: stream } -> case release stream state1 of
      { state: state2, value: _ } -> { state: state2, value: 42 }"#,
        ),
        (
            r#"foreign import data Pollable :: Type
foreign import "wasi:clocks/monotonic-clock#subscribe-duration" subscribe :: Int -> State RealWorld -> Step RealWorld Pollable
foreign import "wasi:io/poll#[resource-drop]pollable" release :: Pollable -> State RealWorld -> Step RealWorld Unit"#,
            r#"case subscribe 0 state0 of
    { state: state1, value: pollable } -> case release pollable state1 of
      { state: state2, value: _ } -> { state: state2, value: 42 }"#,
        ),
        (
            r#"foreign import "wasi:clocks/wall-clock#now" now :: State RealWorld -> Step RealWorld { seconds :: Int, nanoseconds :: Int }
foreign import "psrs:intrinsic#intLt" lt :: Int -> Int -> Boolean"#,
            r#"case now state0 of
    { state: state1, value: time } ->
      { state: state1, value: if lt time.nanoseconds 1000000000 then 42 else 1 }"#,
        ),
    ] {
        let source = source(declarations, action);
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert_eq!(output.status.code(), Some(42), "{output:?}");
    }
}

#[test]
fn source_newtype_host_list_adapter_executes_its_loop_and_cleanup() {
    let source = source(
        r#"foreign import "wasi:cli/environment#get-arguments" arguments :: State RealWorld -> Step RealWorld (Array String)
foreign import "psrs:intrinsic#arrayLength" length :: forall a. Array a -> Int"#,
        r#"case arguments state0 of
    { state: state1, value: items } -> { state: state1, value: length items }"#,
    );
    let Some(output) = run_with_wasmtime_args(&source, &["alpha", "beta"]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(3), "{output:?}");
}

#[test]
fn source_newtype_host_variant_adapter_executes_checked_internal_edges() {
    let source = source(
        r#"foreign import data Stream :: Type
foreign import data Error :: Type
data StreamError = LastOperationFailed Error | Closed
foreign import "wasi:cli/stdout#get-stdout" stdout :: State RealWorld -> Step RealWorld Stream
foreign import "wasi:io/streams#[method]output-stream.flush" flush :: Stream -> State RealWorld -> Step RealWorld (Either StreamError Unit)
foreign import "wasi:io/streams#[resource-drop]output-stream" release :: Stream -> State RealWorld -> Step RealWorld Unit
foreign import "psrs:runtime-storage#array_read" read :: forall s a. Array a -> Int -> State s -> Step s a"#,
        r#"case stdout state0 of
    { state: state1, value: stream } -> case flush stream state1 of
      { state: state2, value: _ } -> case release stream state2 of
        { state: state3, value: _ } -> read [42] 0 state3"#,
    ).replace("import Prim.State", "import Data.Either (Either)\nimport Prim.State");
    let sources = [
        (
            "Either.purs",
            "module Data.Either (Either(..)) where\ndata Either a b = Left a | Right b\n",
        ),
        ("Main.purs", source.as_str()),
    ];
    let Some(output) = run_program_with_wasmtime(&sources) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    let core = lower_program_to_core(&sources).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let (module, _) = psrs_backend::mir::lower_module_with_bindings(
        input.cc,
        input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .unwrap();
    psrs_backend::mir::verify_module(&module).unwrap();
    let mut changed = module.clone();
    let block = changed
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .find(|block| {
            matches!(
                block.terminator,
                Some(psrs_backend::mir::Terminator::Switch { .. })
            )
        })
        .expect("canonical variant result must select its checked case");
    let psrs_backend::mir::Terminator::Switch { default, cases, .. } =
        block.terminator.as_mut().unwrap()
    else {
        panic!()
    };
    *default = cases[0].1;
    let errors = psrs_backend::mir::verify_module(&changed).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P9 canonical call verification"),
        "changing the internal edge must fail canonical correspondence: {errors:?}"
    );
}

#[test]
fn bare_foreign_function_values_use_checked_source_schemes() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import data Stream :: Type
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
foreign import "wasi:cli/stdout#get-stdout" get :: State RealWorld -> Step RealWorld Stream
foreign import "wasi:io/streams#[resource-drop]output-stream" release :: Stream -> State RealWorld -> Step RealWorld Unit
newtype Native a = Native (State RealWorld -> Step RealWorld a)
fromState :: forall a. (State RealWorld -> Step RealWorld a) -> Native a
fromState body = Native body
stdout :: Native Stream
stdout = fromState get
main :: Int
main = case stdout of
  Native call -> execute (\state0 -> case call state0 of
    { state: state1, value: stream } -> case release stream state1 of
      { state: state2, value: _ } -> { state: state2, value: 42 })
"#;
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

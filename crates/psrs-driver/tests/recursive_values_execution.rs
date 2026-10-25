//! Execution contracts for eager recursive local values and escaping captures.
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

fn run(source: &str) -> Option<std::process::Output> {
    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !available {
        assert_ne!(
            std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(),
            Ok("1"),
            "required Wasmtime is unavailable"
        );
        eprintln!("skipping recursive-value execution: Wasmtime unavailable");
        return None;
    }
    let artifact =
        psrs_driver::compile_source("Main.purs", source).expect("recursive-value fixture compiles");
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "psrs-recursive-values-{}-{}.wasm",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, artifact.wasm).expect("write component");
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("execute component");
    std::fs::remove_file(path).expect("remove component");
    Some(output)
}

fn returns(source: &str, code: i32) {
    if let Some(output) = run(source) {
        assert_eq!(output.status.code(), Some(code), "{output:?}");
    }
}

#[test]
fn recursive_data_escapes_through_nested_closures() {
    returns(
        r"module Main where
import Prelude
data Stream a = Stream a (Unit -> Stream a)
head (Stream x _) = x
tail (Stream _ next) = next unit
make n =
  let stream = Stream n (\_ -> stream)
      escape _ = \_ -> tail stream
  in escape unit
main = head ((make 42) unit)
",
        42,
    );
}

#[test]
fn mixed_function_value_group_preserves_eager_once_only_initialization() {
    returns(
        r"module Main where
main =
  let counter = [0]
      value = let written = arrayWrite counter 0 (intAdd (arrayIndex counter 0) 1)
              in { answer: 40, next: \_ -> function 0, observed: arrayIndex written 0 }
      function n = intAdd value.answer n
      one = value.next 0
      two = value.next 0
  in intAdd (intAdd (intSub one two) (intMul value.observed 40)) (intMul (arrayIndex counter 0) 2)
",
        42,
    );
}

#[test]
fn memoized_values_retain_shared_array_identity() {
    returns(
        r"module Main where
main =
  let value = build 0
      build n = if intEq n 0 then [42] else value
      changed = arrayWrite value 0 37
  in intAdd (arrayIndex value 0) (intSub (arrayIndex changed 0) 32)
",
        42,
    );
}

#[test]
fn demand_during_initialization_traps_even_when_body_ignores_value() {
    if let Some(output) = run(r"module Main where
import Prelude
applyThunk :: (Unit -> Int) -> Int
applyThunk thunk = thunk unit
main =
  let value = applyThunk (\_ -> value)
  in 42
")
    {
        assert!(
            !output.status.success(),
            "an eagerly demanded cycle must fail: {output:?}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unreachable"),
            "{output:?}"
        );
    }
}

#[test]
fn callable_alias_is_initialized_before_its_first_call() {
    returns(
        r"module Main where
main =
  let alias = function
      function n = if intEq n 0 then 42 else alias (intSub n 1)
  in alias 2
",
        42,
    );
}

#[test]
fn recursive_function_restores_a_captured_memo_access_mode() {
    returns(
        r"module Main where
import Prelude
data Stream a = Stream a (Unit -> Stream a)
head (Stream x _) = x
make n =
  let stream = Stream n (\_ -> stream)
      loop i = if i == 0 then head stream else loop (i - 1)
  in loop
main = (make 42) 3
",
        42,
    );
}

#[test]
fn successful_scalar_memos_finish_after_recursive_initializer_calls() {
    returns(
        r#"module Main where
import Prelude
choose :: forall a. Int -> a -> (Unit -> a) -> a
choose count initial next =
  if count == 0 then initial else choose (count - 1) initial next
main =
  let i = choose 1 40 (\_ -> i)
      n = choose 1 1.5 (\_ -> n)
      b = choose 1 true (\_ -> b)
      s = choose 1 "λ" (\_ -> s)
  in if n == 1.5 && b && s == "λ" then i + 2 else 0
"#,
        42,
    );
}

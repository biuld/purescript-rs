use super::*;

#[test]
fn runs_a_top_level_function_value_through_call_ref() {
    let source = r#"module Main where
apply :: (Int -> Int) -> Int -> Int
apply f x = f x
inc :: Int -> Int
inc x = x + 1
main = apply inc 41
"#;
    let artifact = compile_source("Main.purs", source).expect("lowering a higher-order call");
    assert!(artifact.wat.contains("ref.func"));
    assert!(artifact.wat.contains("call_ref"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_non_capturing_local_lambda_through_call_ref() {
    let source = r#"module Main where
main = let f = \x -> x + 1 in f 41
"#;
    let artifact = compile_source("Main.purs", source).expect("lowering a local lambda");
    assert!(artifact.wat.contains("ref.func"));
    assert!(artifact.wat.contains("call_ref"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_capturing_lambda_through_a_closure() {
    let source = r#"module Main where
apply :: (Int -> Int) -> Int -> Int
apply f x = f x
main = let x = 40 in apply (\y -> x + y) 2
"#;
    let artifact = compile_source("Main.purs", source).expect("lowering a capturing lambda");
    assert!(artifact.wat.contains("array.new_fixed"));
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn preserves_full_width_ints_in_capturing_lambdas() {
    let source = r#"module Main where
apply :: (Int -> Int) -> Int -> Int
apply f x = f x
main = let captured = 1073741824
       in apply (\ignored -> if captured == 1073741824 then 42 else 1) 0
"#;
    let artifact = compile_source("Main.purs", source).expect("lowering a full-width Int capture");
    assert!(artifact.wat.contains("struct.new"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_record_capturing_lambda_through_a_closure() {
    let source = r#"module Main where
apply :: (Int -> Int) -> Int -> Int
apply f x = f x
main = let r = { answer: 40 } in apply (\ignored -> case r of { answer: value } -> value + 2) 0
"#;
    let artifact = compile_source("Main.purs", source).expect("lowering a record capture");
    assert!(artifact.wat.contains("array.new_fixed"));
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_polymorphic_identity_with_an_integer() {
    let source = r#"module Main where
identity :: forall a. a -> a
identity value = value
main = identity 42
"#;
    let _artifact = compile_source("Main.purs", source).expect("lowering a polymorphic identity");
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_polymorphic_identity_with_a_number() {
    let source = r#"module Main where
identity :: forall a. a -> a
identity value = value
use :: Number -> Int
use value = 42
main = use (identity 1.5)
"#;
    let artifact = compile_source("Main.purs", source).expect("lowering a polymorphic Number");
    assert!(artifact.wat.contains("f64"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_polymorphic_higher_order_call() {
    let source = r#"module Main where
apply :: forall a. (a -> a) -> a -> a
apply f value = f value
main = apply (\value -> value) 42
"#;
    let artifact = compile_source("Main.purs", source).expect("lowering a polymorphic call_ref");
    assert!(artifact.wat.contains("call_ref"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn adapts_a_concrete_lambda_to_a_partially_erased_function() {
    let source = r#"module Main where
applyValue :: forall a. (a -> Int) -> a -> Int
applyValue f value = f value
main = applyValue (\value -> 42) 0
"#;
    let artifact =
        compile_source("Main.purs", source).expect("lowering a partially erased call_ref");
    assert!(artifact.wat.contains("call_ref"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn invokes_a_concrete_function_returned_from_a_polymorphic_call() {
    let source = r#"module Main where
identity :: forall a. a -> a
identity value = value
main = let f = identity (\value -> value) in f 42
"#;
    let artifact =
        compile_source("Main.purs", source).expect("lowering a polymorphic function result");
    assert!(artifact.wat.contains("call_ref"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn invokes_a_polymorphic_global_function_value_at_a_concrete_type() {
    let source = r#"module Main where
identity :: forall a. a -> a
identity value = value
main = let f = identity in f 42
"#;
    let artifact =
        compile_source("Main.purs", source).expect("lowering a polymorphic global value");
    assert!(artifact.wat.contains("call_ref"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_recursive_polymorphic_reference_identity() {
    // P10 can rewrite the `if` into a jump that leaves the other arm
    // unreachable while it still names the entry block's array value. The
    // optimizer must prune that dead arm before verifying.
    let source = r#"module Main where
lastArr :: forall a. Int -> Array a -> Array a
lastArr n x = if n == 0 then x else lastArr (n - 1) x
main = arrayIndex (lastArr 3 [40, 42]) 1
"#;
    let _artifact =
        compile_source("Main.purs", source).expect("lowering a recursive reference arm");
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_recursive_polymorphic_identity_at_integer_types() {
    let source = r#"module Main where
f :: forall a. Int -> a -> a
f n x = if n == 0 then x else f (n - 1) x
main = f 1 42 + f 2 0
"#;
    let _artifact = compile_source("Main.purs", source).expect("lowering a recursive identity");
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_branch_with_equal_reference_arms() {
    // Both arms pass the same reference, so constant propagation can
    // materialize the merge parameter. The one-value merge contract must be
    // preserved.
    let source = r#"module Main where
g :: forall a. Int -> a -> a
g n x = if n == 0 then x else x
main = g 1 42
"#;
    let _artifact = compile_source("Main.purs", source).expect("lowering equal reference arms");
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_case_that_returns_a_reference() {
    // The unreachable default arm traps; the structurer must not read the
    // trap destination local, which is uninitialized for a reference result.
    let source = r#"module Main where
data Maybe a = Nothing | Just a
fromJust :: forall a. Maybe a -> a -> a
fromJust m d = case m of
  Nothing -> d
  Just x -> x
main = fromJust (Just 42) 0
"#;
    let _artifact = compile_source("Main.purs", source).expect("lowering a reference case");
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

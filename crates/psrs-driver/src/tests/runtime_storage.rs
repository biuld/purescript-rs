use super::*;

const IMPORTS: &str = r#"
foreign import "psrs:runtime-storage#array_fill" filled :: forall a. Int -> a -> Array a
foreign import "psrs:runtime-storage#array_read" read :: forall a. Array a -> Int -> a
foreign import "psrs:runtime-storage#array_write" write :: forall a. Array a -> Int -> a -> Unit
"#;

#[test]
fn runtime_storage_preserves_aliases_and_recovers_payloads() {
    let source = format!(
        r#"module Main where
{IMPORTS}
main =
  let
    xs = filled 2 1
    alias = xs
    put = write xs 1
    ignored = put 42
    texts = filled 1 "😀"
    records = filled 1 {{ value: 42 }}
    functions = filled 1 (\x -> intAdd x 2)
    nested = filled 1 (filled 1 42)
  in if booleanAnd (intEq (read alias 1) 42)
    (booleanAnd (intEq (arrayLength (stringToBytes (read texts 0))) 4)
    (booleanAnd (intEq (read records 0).value 42)
    (booleanAnd (intEq ((read functions 0) 40) 42)
    (intEq (read (read nested 0) 0) 42)))) then 42 else 1
"#
    );
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn runtime_storage_validates_unused_source_contracts() {
    for (export, ty) in [
        ("array_fill", "Int -> Int -> Array Number"),
        ("array_read", "forall a b. Array a -> Int -> b"),
        ("array_write", "forall a. Array a -> Int -> a -> Int"),
        ("array_write", "Array Int -> Int -> Number -> Unit"),
        ("array_read", "forall a. Array a -> Number -> a"),
        ("array_fill", "forall a. a -> Array a"),
        ("array_read", "Array Boolean -> Int -> Int"),
    ] {
        let source = format!(
            "module Main where\nforeign import \"psrs:runtime-storage#{export}\" op :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("invalid storage contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 runtime linking"),
            "{export}: {errors:?}"
        );
    }
}

#[test]
fn runtime_storage_valid_unused_imports_do_not_link_a_provider() {
    let source = format!("module Main where\n{IMPORTS}\nmain = 42\n");
    let unused = compile_program_sources(&[("Main.purs", &source)]).unwrap();
    assert!(!unused.wat.contains("array_fill"), "{}", unused.wat);
    let source = format!("module Main where\n{IMPORTS}\nmain = read (filled 1 42) 0\n");
    let used = compile_program_sources(&[("Main.purs", &source)]).unwrap();
    assert!(used.wat.contains("array_fill"), "{}", used.wat);
}

#[test]
fn runtime_storage_traps_on_invalid_ranges() {
    for (body, trap) in [
        ("arrayLength (filled (intNeg 1) 0)", "unreachable"),
        ("read (filled 1 0) 1", "out of bounds array access"),
        (
            "let ignored = write (filled 1 0) 1 42 in 0",
            "out of bounds array access",
        ),
    ] {
        let source = format!("module Main where\n{IMPORTS}\nmain = {body}\n");
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert!(!output.status.success(), "{body}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(trap),
            "{output:?}"
        );
    }
}

#[test]
fn runtime_storage_trap_has_no_normal_return() {
    for ty in ["Int", "forall a. a"] {
        let source = format!(
            "module Main where\nforeign import \"psrs:runtime-storage#trap\" abort :: {ty}\nmain :: Int\nmain = abort\n"
        );
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert!(!output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unreachable"),
            "{output:?}"
        );
    }
}

#[test]
fn partial_crash_with_reaches_the_storage_trap() {
    // `Partial._crashWith` is an official JavaScript `foreign import` that
    // throws. The target implements it with the checked non-returning storage
    // trap, so a real library caller must never return a value for the absent
    // result type.
    let source = "module Main where\nimport Prelude\nimport Partial.Unsafe (unsafeCrashWith)\nmain :: Int\nmain = unsafeCrashWith \"boom\"\n";
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unreachable"),
        "{output:?}"
    );
}

#[test]
fn runtime_storage_monomorphic_operands_use_the_same_array() {
    let source = r#"module Main where
foreign import "psrs:runtime-storage#array_fill" filled :: Int -> Int -> Array Int
foreign import "psrs:runtime-storage#array_read" read :: Array Int -> Int -> Int
foreign import "psrs:runtime-storage#array_write" write :: Array Int -> Int -> Int -> Unit
main = let xs = filled 1 0
           ignored = write xs 0 42
       in read xs 0
"#;
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn static_cells_share_per_binding_and_preserve_independent_bindings() {
    let source = format!(
        r#"module Main where
{IMPORTS}
foreign import "psrs:runtime-static#cell" first :: Int -> Array Int
foreign import "psrs:runtime-static#cell" second :: Int -> Array Int
getFirst :: Unit -> Array Int
getFirst _ = first 0
main = let ignored = write (getFirst unit) 0 42
           other = write (second 0) 0 7
       in if intEq (read (second 100) 0) 7 then read (getFirst unit) 0 else 1
"#
    );
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn static_cells_reject_invalid_unused_source_contracts() {
    for ty in [
        "forall a. a -> Array a",
        "Int -> Array Number",
        "(forall a. a -> a) -> Array (forall a. a -> a)",
        "Int -> Int",
    ] {
        let source = format!(
            "module Main where\nforeign import \"psrs:runtime-static#cell\" cell :: {ty}\nmain = 0\n"
        );
        let errors =
            compile_program_sources(&[("Main.purs", &source)]).expect_err("invalid static cell");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 runtime linking"),
            "{ty}: {errors:?}"
        );
    }
}

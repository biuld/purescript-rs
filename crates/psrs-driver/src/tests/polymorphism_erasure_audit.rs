//! Value-sensitive execution and interning evidence for the Polymorphism and
//! Erasure topic (PE-01..PE-11). Every case here is a source program that runs
//! through the normal pipeline and is executed with Wasmtime when it is
//! available.

use super::*;
use crate::program::lower_program_to_core;
use psrs_backend::mir::Instruction;
use psrs_backend::types::{CompositeType, RecGroup, ValueType};
use std::collections::HashSet;

fn required_wasmtime() -> bool {
    std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1")
}

fn execute_component(name: &str, wasm: &[u8]) -> Result<Option<std::process::Output>, String> {
    let available = std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available {
        if required_wasmtime() {
            return Err(format!("wasmtime is unavailable for `{name}`"));
        }
        eprintln!("skipping `{name}`: wasmtime is not installed");
        return Ok(None);
    }
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "psrs-polymorphism-erasure-{}-{id}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Ok(Some(output))
}

fn run_source(name: &str, source: &str) -> Result<Option<i32>, String> {
    let artifact = match compile_source("Main.purs", source) {
        Ok(artifact) => artifact,
        Err(errors) => return Err(format!("`{name}` failed to compile: {errors:?}")),
    };
    execute_component(name, &artifact.wasm)
        .map(|output| output.map(|result| result.status.code().unwrap_or(-1)))
}

fn expect_exit(name: &str, source: &str, expected: i32) {
    match run_source(name, source) {
        Ok(Some(code)) => assert_eq!(code, expected, "`{name}` produced the wrong exit code"),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

fn pre_optimization_mir(source: &str) -> psrs_backend::mir::Module {
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let backend_input = psrs_backend::cc::lower_module(core).expect("Core should lower to CC");
    psrs_backend::mir::lower_module_with_bindings(
        backend_input.cc,
        backend_input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .expect("CC should lower to MIR")
    .0
}

fn function_type_keys(types: &[RecGroup]) -> Vec<(Vec<ValueType>, Vec<ValueType>)> {
    types
        .iter()
        .flat_map(|group| &group.0)
        .filter_map(|defined| match &defined.composite {
            CompositeType::Func {
                parameters,
                results,
            } => Some((parameters.clone(), results.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn equal_normalized_function_signatures_allocate_one_mir_function_type() {
    let source = "module Main where\nfInt :: Array Int -> Array Int\nfInt values = values\nfStr :: Array String -> Array String\nfStr values = values\nuseInt :: (Array Int -> Array Int) -> Int\nuseInt function = arrayLength (function [1, 2])\nuseStr :: (Array String -> Array String) -> Int\nuseStr function = arrayLength (function [\"a\", \"b\"])\nmain = useInt fInt + useStr fStr\n";
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let stages =
        psrs_backend::compile_with_stages(core).expect("the two function types should compile");
    let keys = function_type_keys(&stages.mir.types);
    assert!(
        !keys.is_empty(),
        "the program must allocate at least one closure function type"
    );
    let distinct = keys.iter().collect::<HashSet<_>>();
    assert_eq!(
        keys.len(),
        distinct.len(),
        "equal normalized signatures must not allocate duplicate MIR function types: {keys:?}"
    );
    match run_source("equal_normalized_signatures", source) {
        Ok(Some(code)) => assert_eq!(code, 4),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn erased_int_box_preserves_high_bit_values() {
    let source = "module Main where\nidentity :: forall a. a -> a\nidentity value = value\nmain = if identity 2000000000 == 2000000000 then 42 else 1\n";
    expect_exit("erased_int_high_bit", source, 42);
}

#[test]
fn erased_boolean_box_preserves_both_values() {
    let source = "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = if arrayIndex (copy [true, false]) 1 then 1 else 42\n";
    expect_exit("erased_boolean_both", source, 42);
}

#[test]
fn erased_string_box_preserves_nonempty_contents() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = let ignored = runEffect (log (arrayIndex (copy [\"high\", \"value\"]) 1)) in 0\n";
    let artifact = compile_source("Main.purs", source).expect("string round trip should compile");
    match execute_component("erased_string_contents", &artifact.wasm) {
        Ok(Some(output)) => {
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, b"value\n");
        }
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn erased_number_box_preserves_negative_values() {
    let source = "module Main where\nidentity :: forall a. a -> a\nidentity value = value\nmain = if numberEq (identity (numberNeg 1.5)) (numberNeg 1.5) then 42 else 1\n";
    expect_exit("erased_number_negative", source, 42);
}

#[test]
fn erased_number_box_preserves_nan_and_signed_zero() {
    let nan = "module Main where\nidentity :: forall a. a -> a\nidentity value = value\nmain = if numberEq (identity (numberDiv 0.0 0.0)) (numberDiv 0.0 0.0) then 1 else 42\n";
    expect_exit("erased_number_nan", nan, 42);
    let signed_zero = "module Main where\nidentity :: forall a. a -> a\nidentity value = value\nmain = if numberLt (numberDiv 1.0 (identity (numberNeg 0.0))) 0.0 then 42 else 1\n";
    expect_exit("erased_number_signed_zero", signed_zero, 42);
}

#[test]
fn erased_identity_boxes_char_and_unit() {
    let char_source = "module Main where\nidentity :: forall a. a -> a\nidentity value = value\nmain = charToInt (identity 'B')\n";
    expect_exit("erased_char", char_source, 66);
    let unit_source = "module Main where\nimport Prelude\nimport WASI.Console\nidentity :: forall a. a -> a\nidentity value = value\nmain = let ignored = identity (runEffect (log \"\")) in 42\n";
    expect_exit("erased_unit", unit_source, 42);
}

#[test]
fn boolean_capture_uses_i31_and_round_trips() {
    let source = "module Main where\nmakeReader :: Boolean -> (Int -> Int)\nmakeReader flag = let captured = flag in \\index -> if captured then index else 0\nmain = let trueReader = makeReader true in let falseReader = makeReader false in trueReader 42 + falseReader 42\n";
    let mir = pre_optimization_mir(source);
    let mut capture_types = Vec::new();
    for function in &mir.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                let Instruction::ClosureNew { captures, .. } = instruction else {
                    continue;
                };
                for capture in captures {
                    if let Some(value) = function.values.iter().find(|value| value.id == *capture) {
                        capture_types.push(value.ty);
                    }
                }
            }
        }
    }
    assert!(
        capture_types.contains(&ValueType::Boolean),
        "a Boolean closure capture must be a Boolean-typed slot so P9 uses i31: {capture_types:?}"
    );
    expect_exit("boolean_capture_i31", source, 42);
}

#[test]
fn escaping_closures_capture_int_and_reference_values() {
    let source = "module Main where\nmakeReader :: Int -> Array Int -> (Int -> Int)\nmakeReader base values = \\index -> base + arrayIndex values 0 + index\nmain = let reader = makeReader 40 [1, 2] in reader 2\n";
    let mir = pre_optimization_mir(source);
    let mut capture_types = Vec::new();
    for function in &mir.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                let Instruction::ClosureNew { captures, .. } = instruction else {
                    continue;
                };
                for capture in captures {
                    if let Some(value) = function.values.iter().find(|value| value.id == *capture) {
                        capture_types.push(value.ty);
                    }
                }
            }
        }
    }
    assert!(
        capture_types.contains(&ValueType::I32),
        "an Int capture must use the integer-shaped slot: {capture_types:?}"
    );
    assert!(
        capture_types
            .iter()
            .any(|ty| matches!(ty, ValueType::Ref(_))),
        "a reference capture must be stored as a reference slot: {capture_types:?}"
    );
    expect_exit("escaping_int_reference_capture", source, 43);
}

#[test]
fn escaping_closures_capture_a_string_value() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmakeReader :: String -> (Int -> String)\nmakeReader message = \\index -> if index == 0 then message else message\nmain = let reader = makeReader \"captured\" in let ignored = runEffect (log (reader 0)) in 42\n";
    let artifact = compile_source("Main.purs", source).expect("string capture should compile");
    match execute_component("escaping_string_capture", &artifact.wasm) {
        Ok(Some(output)) => {
            assert_eq!(output.stdout, b"captured\n");
            assert_eq!(output.status.code(), Some(42));
        }
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn higher_order_adapters_are_value_sensitive_in_both_directions() {
    let generic_to_concrete = "module Main where\napplyInt :: forall a. (a -> a) -> a -> a\napplyInt function value = function value\nconcreteInt :: Int -> Int\nconcreteInt value = value + 1\nmain = applyInt concreteInt 41\n";
    expect_exit("adapter_generic_to_concrete_int", generic_to_concrete, 42);
    let boolean_instance = "module Main where\napplyValue :: forall a. (a -> a) -> a -> a\napplyValue function value = function value\nflip :: Boolean -> Boolean\nflip value = if value then false else true\nmain = if applyValue flip true then 1 else 42\n";
    expect_exit("adapter_generic_to_concrete_boolean", boolean_instance, 42);
    let concrete_to_generic = "module Main where\napplyConcrete :: (Int -> Int) -> Int -> Int\napplyConcrete function value = function value\ngeneric :: forall a. a -> a\ngeneric value = value\nmain = applyConcrete generic 42\n";
    expect_exit("adapter_concrete_to_generic_int", concrete_to_generic, 42);
}

#[test]
fn linked_modules_round_trip_an_erased_high_bit_int() {
    let producer = (
        "Producer.purs",
        "module Producer where\nidentity :: forall a. a -> a\nidentity value = value\n",
    );
    let consumer = (
        "Main.purs",
        "module Main where\nimport Producer\nmain = identity 2000000000 - 1999999958\n",
    );
    let core = lower_program_to_core(&[producer, consumer])
        .expect("the linked erased program should lower to Core");
    let stages =
        psrs_backend::compile_with_stages(core).expect("the linked erased program should compile");
    let keys = function_type_keys(&stages.mir.types);
    let distinct = keys.iter().collect::<HashSet<_>>();
    assert_eq!(keys.len(), distinct.len());
    match execute_component("linked_erased_high_bit", &stages.artifact.wasm) {
        Ok(Some(output)) => assert_eq!(output.status.code(), Some(42), "{output:?}"),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

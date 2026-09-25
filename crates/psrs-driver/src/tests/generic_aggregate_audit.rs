use super::*;
use crate::program::lower_program_to_core;

struct Case {
    name: &'static str,
    source: &'static str,
    exit: i32,
}

fn required_wasmtime() -> bool {
    std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1")
}

fn run_required(name: &str, source: &str) -> Result<Option<i32>, String> {
    let artifact = match compile_source("Main.purs", source) {
        Ok(artifact) => artifact,
        Err(errors) => return Err(format!("`{name}` failed to compile: {errors:?}")),
    };
    execute_wasm(name, &artifact.wasm)
}

fn execute_wasm(name: &str, wasm: &[u8]) -> Result<Option<i32>, String> {
    execute_wasm_output(name, wasm)
        .map(|output| output.map(|result| result.status.code().unwrap_or(-1)))
}

fn execute_wasm_output(name: &str, wasm: &[u8]) -> Result<Option<std::process::Output>, String> {
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
        "psrs-generic-aggregate-audit-{}-{id}.wasm",
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

#[test]
fn generic_string_array_preserves_observable_contents() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = let ignored = runEffect (log (arrayIndex (copy [\"hello\", \"world\"]) 1)) in 0\n";
    let artifact = compile_source("Main.purs", source).expect("string round trip should compile");
    match execute_wasm_output("generic_string_contents", &artifact.wasm) {
        Ok(Some(output)) => {
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, b"world\n");
        }
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn linked_modules_share_canonical_layouts() {
    let producer = (
        "Producer.purs",
        "module Producer where\ndata Wrap a = Wrap (Array a)\nwrap :: forall a. Array a -> Wrap a\nwrap values = Wrap values\nunwrap :: forall a. Wrap a -> Array a\nunwrap value = case value of\n  Wrap values -> values\n",
    );
    let consumer = (
        "Main.purs",
        "module Main where\nimport Producer\nmain = arrayIndex (unwrap (wrap [40, 42])) 1\n",
    );
    let core = lower_program_to_core(&[producer, consumer])
        .expect("linked modules with a generic producer should lower to Core");
    let stages = psrs_backend::compile_with_stages(core)
        .expect("linked generic boundaries should compile through the normal pipeline");
    let erased = psrs_backend::cc::ValueShape::Reference(psrs_backend::cc::Reference {
        nullable: false,
        heap: psrs_backend::cc::RefShape::Erased,
    });
    let canonical_arrays = stages
        .cc
        .representations
        .representations
        .iter()
        .filter(|representation| {
            matches!(representation, psrs_backend::cc::Representation::Array { element } if *element == erased)
        })
        .count();
    assert_eq!(
        canonical_arrays, 1,
        "linked modules must share one canonical Array(Erased) layout"
    );
    assert!(
        array_new_default_count(&stages.mir) >= 1,
        "the linked producer boundary must retain a reconstruction path"
    );
    match execute_wasm("linked_modules", &stages.artifact.wasm) {
        Ok(Some(code)) => assert_eq!(code, 42, "linked round-trip should preserve its element"),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn retained_generic_capture_executes_through_a_closure() {
    let generic_array = "module Main where\nmakeReader :: forall a. Array a -> (Int -> a)\nmakeReader values = let a = arrayUpdate values 0 (arrayIndex values 0) in let b = arrayUpdate a 0 (arrayIndex a 0) in let c = arrayUpdate b 0 (arrayIndex b 0) in let d = arrayUpdate c 0 (arrayIndex c 0) in let e = arrayUpdate d 0 (arrayIndex d 0) in let f = arrayUpdate e 0 (arrayIndex e 0) in let g = arrayUpdate f 0 (arrayIndex f 0) in let h = arrayUpdate g 0 (arrayIndex g 0) in \\index -> arrayIndex h index\nmain = let reader = makeReader [40, 42] in reader 1\n";
    let concrete_array = generic_array
        .replace("forall a. Array a", "Array Int")
        .replace("(Int -> a)", "(Int -> Int)");
    let generic_record = "module Main where\nmakeReader :: forall a. { value :: a, count :: Int } -> (Int -> a)\nmakeReader record = let a = record { count = record.count + 1 } in let b = a { count = a.count + 1 } in let c = b { count = b.count + 1 } in let d = c { count = c.count + 1 } in let e = d { count = d.count + 1 } in let f = e { count = e.count + 1 } in let g = f { count = f.count + 1 } in let h = g { count = g.count + 1 } in \\index -> h.value\nmain = let reader = makeReader { value: [40, 42], count: 0 } in arrayIndex (reader 0) 1\n";
    for (name, source) in [
        ("generic_array_capture", generic_array),
        ("concrete_array_capture", concrete_array.as_str()),
        ("generic_record_capture", generic_record),
    ] {
        assert_retained_capture(name, source);
    }
}

fn assert_retained_capture(name: &str, source: &str) {
    let core = lower_source_to_core("Main.purs", source).expect("closure source should lower");
    let stages = psrs_backend::compile_with_stages(core).expect("closure should compile");
    assert!(
        stages
            .mir
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(
                instruction,
                psrs_backend::mir::Instruction::ClosureNew { .. }
            )),
        "{name} must reach MIR as an allocated closure"
    );
    match execute_wasm(name, &stages.artifact.wasm) {
        Ok(Some(code)) => assert_eq!(code, 42),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn p9_reuses_helpers_for_equal_complete_conversion_plans() {
    let source = "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = arrayIndex (copy [1, 2]) 0 + arrayIndex (copy [3, 4]) 1\n";
    let core = lower_source_to_core("Main.purs", source).unwrap();
    let backend_input = psrs_backend::cc::lower_module(core).unwrap();
    let (mir, _) = psrs_backend::mir::lower_module_with_bindings(
        backend_input.cc,
        backend_input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .unwrap();
    let helpers = mir
        .functions
        .iter()
        .filter(|function| function.name.starts_with("aggregate_convert_"))
        .collect::<Vec<_>>();
    assert!(
        !helpers.is_empty(),
        "generic calls should require conversion helpers"
    );
    let shared = helpers.iter().any(|helper| {
        mir.functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                matches!(instruction,
                psrs_backend::mir::Instruction::Call { function, .. } if *function == helper.symbol)
            })
            .count()
            >= 2
    });
    assert!(
        shared,
        "identical conversion plans should call one shared helper"
    );
    match run_required("shared_conversion_helper", source) {
        Ok(Some(code)) => assert_eq!(code, 5),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn audit_battery() {
    let cases: &[Case] = &[
        // GA-04 arrays both directions, empty / singleton / multi / nested / records
        Case {
            name: "array_roundtrip_multi",
            source: "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = arrayIndex (copy [40, 42]) 1\n",
            exit: 42,
        },
        Case {
            name: "array_roundtrip_singleton",
            source: "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = arrayIndex (copy [42]) 0\n",
            exit: 42,
        },
        Case {
            name: "array_roundtrip_nested",
            source: "module Main where\nduplicate :: forall a. Array (Array a) -> Array (Array a)\nduplicate values = values\nmain = arrayIndex (arrayIndex (duplicate [[40, 42]]) 0) 1\n",
            exit: 42,
        },
        Case {
            name: "array_roundtrip_records",
            source: "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = case arrayIndex (copy [{ answer: 42 }]) 0 of\n  { answer: value } -> value\n",
            exit: 42,
        },
        // GA-05 records canonical order, mixed scalar/reference, nested
        Case {
            name: "record_roundtrip_mixed",
            source: "module Main where\ncopy :: forall a. { value :: a, count :: Int } -> { value :: a, count :: Int }\ncopy record = record { count = record.count }\nmain = arrayIndex ((copy { value: [40, 42], count: 7 }).value) 1\n",
            exit: 42,
        },
        Case {
            name: "record_roundtrip_reordered",
            source: "module Main where\nswap :: forall a. { zed :: a, alpha :: Int } -> { alpha :: Int, zed :: a }\nswap record = { alpha: record.alpha, zed: record.zed }\nmain = arrayIndex ((swap { zed: [40, 42], alpha: 7 }).zed) 1\n",
            exit: 42,
        },
        Case {
            name: "record_roundtrip_nested",
            source: "module Main where\ncopy :: forall a. { inner :: { value :: a } } -> { inner :: { value :: a } }\ncopy record = record\nmain = arrayIndex (copy { inner: { value: [40, 42] } }.inner.value) 1\n",
            exit: 42,
        },
        // GA-06 dependent ADT fields, multiple instantiations
        Case {
            name: "dependent_adt_int",
            source: "module Main where\ndata Wrap a = Wrap (Array a)\nwrap :: forall a. Array a -> Wrap a\nwrap values = Wrap values\nunwrap :: forall a. Wrap a -> Array a\nunwrap value = case value of\n  Wrap values -> values\nmain = arrayIndex (unwrap (wrap [40, 42])) 1\n",
            exit: 42,
        },
        Case {
            name: "dependent_adt_number",
            source: "module Main where\ndata Wrap a = Wrap (Array a)\nwrap :: forall a. Array a -> Wrap a\nwrap values = Wrap values\nunwrap :: forall a. Wrap a -> Array a\nunwrap value = case value of\n  Wrap values -> values\nmain = if numberEq (arrayIndex (unwrap (wrap [1.5, 2.5])) 1) 2.5 then 42 else 1\n",
            exit: 42,
        },
        Case {
            name: "dependent_adt_record",
            source: "module Main where\ndata Wrap a = Wrap (Array a)\nwrap :: forall a. Array a -> Wrap a\nwrap values = Wrap values\nunwrap :: forall a. Wrap a -> Array a\nunwrap value = case value of\n  Wrap values -> values\nmain = case arrayIndex (unwrap (wrap [{ answer: 42 }])) 0 of\n  { answer: value } -> value\n",
            exit: 42,
        },
        // GA-07 bare-variable ADT field
        Case {
            name: "bare_variable_adt_array",
            source: "module Main where\ndata Hold a = Hold a\nhold :: forall a. a -> Hold a\nhold value = Hold value\nunhold :: forall a. Hold a -> a\nunhold value = case value of\n  Hold inner -> inner\nmain = arrayIndex (unhold (hold [40, 42])) 1\n",
            exit: 42,
        },
        // GA-08 array literal/read/update, aliasing
        Case {
            name: "array_update_alias",
            source: "module Main where\nmain = let original = [10, 20] in let alias = original in let updated = arrayUpdate original 0 99 in arrayIndex alias 0 + arrayIndex updated 0\n",
            exit: 109,
        },
        // GA-09 record construction/access/pattern/update, aliasing
        Case {
            name: "record_update_alias",
            source: "module Main where\nmain = let original = { answer: 10 } in let alias = original in let updated = original { answer = 42 } in alias.answer + updated.answer\n",
            exit: 52,
        },
        // GA-10 direct call args and returns at distinct concrete types
        Case {
            name: "direct_call_distinct_int",
            source: "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = arrayIndex (copy [40, 42]) 1\n",
            exit: 42,
        },
        Case {
            name: "direct_call_distinct_number",
            source: "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = if numberEq (arrayIndex (copy [1.5, 2.5]) 0) 1.5 then 42 else 1\n",
            exit: 42,
        },
        // GA-11 higher-order adapters
        Case {
            name: "higher_order_array",
            source: "module Main where\napplyArray :: forall a. (Array a -> Array a) -> Array a -> Array a\napplyArray function values = function values\nconcrete :: Array Int -> Array Int\nconcrete values = values\nmain = arrayIndex (applyArray concrete [40, 42]) 1\n",
            exit: 42,
        },
        Case {
            name: "higher_order_nested_array",
            source: "module Main where\napplyArray :: forall a. (Array a -> Array a) -> Array a -> Array a\napplyArray function values = function values\nconcrete :: Array (Array Int) -> Array (Array Int)\nconcrete values = values\nmain = arrayIndex (arrayIndex (applyArray concrete [[40, 42]]) 0) 1\n",
            exit: 42,
        },
        Case {
            name: "higher_order_generic_to_concrete",
            source: "module Main where\napplyConcrete :: (Array Int -> Array Int) -> Array Int -> Array Int\napplyConcrete function values = function values\ngeneric :: forall a. Array a -> Array a\ngeneric values = values\nmain = arrayIndex (applyConcrete generic [40, 42]) 1\n",
            exit: 42,
        },
        Case {
            name: "higher_order_mixed_parameters",
            source: "module Main where\napplyMixed :: forall a. (Array a -> Int -> Array a) -> Array a -> Int -> Array a\napplyMixed function values index = function values index\nconcrete :: Array Int -> Int -> Array Int\nconcrete values index = arrayUpdate values index 42\nmain = arrayIndex (applyMixed concrete [40, 41] 1) 1\n",
            exit: 42,
        },
        // GA-12 captures
        Case {
            name: "capture_generic_array",
            source: "module Main where\ncaptureRead :: forall a. Array a -> Int\ncaptureRead values = (\\index -> arrayLength values) 0\nmain = captureRead [40, 42]\n",
            exit: 2,
        },
        // Scalar coverage across one generic body: Boolean, Char, String, Number, Int
        Case {
            name: "scalar_boolean_array",
            source: "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = if arrayIndex (copy [true, false]) 0 then 42 else 1\n",
            exit: 42,
        },
        Case {
            name: "scalar_char_array",
            source: "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = charToInt (arrayIndex (copy ['A']) 0)\n",
            exit: 65,
        },
        Case {
            name: "scalar_string_array",
            source: "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = arrayLength (copy [\"hello\", \"world\"])\n",
            exit: 2,
        },
        Case {
            name: "scalar_unit_array",
            source: "module Main where\nimport Prelude\nimport WASI.Console\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = arrayLength (copy [runEffect (log \"\")])\n",
            exit: 1,
        },
        Case {
            name: "two_instantiations_share_body",
            source: "module Main where\nsize :: forall a. Array a -> Int\nsize values = arrayLength values\nmain = size [1, 2, 3] + size [1.5, 2.5]\n",
            exit: 5,
        },
        Case {
            name: "two_instantiations_roundtrip",
            source: "module Main where\ncopy :: forall a. Array a -> Array a\ncopy values = values\nmain = if numberEq (arrayIndex (copy [3.5, 4.5]) 0) 3.5 then arrayIndex (copy [1, 2]) 0 + 42 else 1\n",
            exit: 43,
        },
        // Construction and pure updates inside a generic body
        Case {
            name: "generic_literal_construction",
            source: "module Main where\nsingleton :: forall a. a -> Array a\nsingleton value = [value]\nmain = arrayIndex (singleton 42) 0\n",
            exit: 42,
        },
        Case {
            name: "generic_array_update",
            source: "module Main where\nbump :: forall a. Array a -> Array a\nbump values = arrayUpdate values 0 (arrayIndex values 0)\nmain = arrayIndex (bump [42]) 0\n",
            exit: 42,
        },
        Case {
            name: "generic_record_construction",
            source: "module Main where\nwrap :: forall a. a -> { value :: a }\nwrap value = { value: value }\nmain = arrayIndex ((wrap [40, 42]).value) 1\n",
            exit: 42,
        },
        Case {
            name: "generic_record_update",
            source: "module Main where\ncopy :: forall a. { value :: a, count :: Int } -> { value :: a, count :: Int }\ncopy record = record { count = 42 }\nmain = (copy { value: [40], count: 7 }).count\n",
            exit: 42,
        },
        Case {
            name: "generic_boolean_record_field",
            source: "module Main where\nselect :: forall a. { flag :: Boolean, value :: a } -> a\nselect record = if record.flag then record.value else record.value\nmain = arrayIndex (select { flag: true, value: [40, 42] }) 1\n",
            exit: 42,
        },
        Case {
            name: "generic_record_pattern_projection",
            source: "module Main where\nget :: forall a. { value :: a, count :: Int } -> a\nget { value: value, count: _ } = value\nmain = arrayIndex (get { value: [40, 42], count: 1 }) 1\n",
            exit: 42,
        },
        Case {
            name: "generic_record_two_instantiations",
            source: "module Main where\nfirst :: forall a. { value :: a, count :: Int } -> a\nfirst record = record.value\nlengthOf :: Array Number -> Int\nlengthOf values = arrayLength values\nmain = arrayIndex (first { value: [40, 42], count: 1 }) 1 + lengthOf (first { value: [3.5, 4.5, 5.5], count: 2 })\n",
            exit: 45,
        },
    ];

    let mut failures = Vec::new();
    let mut executed = 0;
    for case in cases {
        match run_required(case.name, case.source) {
            Ok(Some(code)) if code == case.exit => executed += 1,
            Ok(Some(code)) => failures.push(format!(
                "{}: expected exit {} but got {}",
                case.name, case.exit, code
            )),
            Ok(None) => failures.push(format!("{}: runtime unavailable", case.name)),
            Err(error) => failures.push(error),
        }
    }
    assert!(
        failures.is_empty(),
        "{executed} cases executed; failures: {failures:#?}"
    );
}

#[path = "generic_aggregate_fixtures.rs"]
mod fixtures;
use fixtures::{array_new_default_count, clear_array_literals};

#[test]
fn empty_array_reconstruction_executes() {
    let source = "module Main where\ndata Wrap a = Wrap (Array a)\nwrap :: forall a. Array a -> Wrap a\nwrap values = Wrap values\nunwrap :: forall a. Wrap a -> Array a\nunwrap value = case value of\n  Wrap values -> values\nmain = arrayLength (unwrap (wrap [0]))\n";
    let mut core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let mut cleared = false;
    for declaration in &mut core.declarations {
        cleared |= clear_array_literals(&mut declaration.value);
    }
    assert!(
        cleared,
        "the fixture should contain an array literal to empty"
    );
    let stages = psrs_backend::compile_with_stages(core)
        .expect("an empty concrete array must still lower through the conversion path");
    assert!(
        array_new_default_count(&stages.mir) >= 1,
        "the empty array should still allocate its canonical destination"
    );
    assert!(
        stages
            .mir
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(
                instruction,
                psrs_backend::mir::Instruction::ArraySet { .. }
            )),
        "the conversion loop should retain its element store even for an empty source"
    );
    match execute_wasm("empty_array_reconstruction", &stages.artifact.wasm) {
        Ok(Some(code)) => assert_eq!(code, 0, "empty array round-trip should have length zero"),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

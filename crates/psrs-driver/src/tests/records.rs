use super::*;

#[test]
fn runs_a_tuple_as_a_closed_record() {
    let source = "\
module Main where
pair :: (Int, Int)
pair = (40, 2)
sum (x, y) = x + y
main = case pair of
  (x, y) -> sum (x, y)
";
    let core = lower_source_to_core("Main.purs", source).expect("tuple should lower to Core");
    let main = core
        .declarations
        .iter()
        .find(|declaration| declaration.name == "main")
        .expect("Core should retain main");
    let psrs_core::ExprKind::Case { scrutinee, .. } = &main.value.kind else {
        panic!("main should case on the tuple");
    };
    let psrs_core::Type::Record(fields) = &core.types[scrutinee.ty.0 as usize] else {
        panic!(
            "a tuple type should be a closed record in Core, got {:?}",
            core.types[scrutinee.ty.0 as usize]
        );
    };
    assert_eq!(
        fields
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["_1", "_2"]
    );
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_record_field_access_through_a_gc_struct() {
    let source = "module Main where\nmain = { ignored: 10, answer: 42 }.answer\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a record field access");
    assert!(artifact.wasm.len() > 8);
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn evaluates_record_fields_in_source_order_before_canonical_layout() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let record = { z: runEffect (log \"z\"), a: runEffect (log \"a\") } in 0\n";
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let main = core
        .declarations
        .iter()
        .find(|declaration| declaration.name == "main")
        .expect("Core should retain main");
    let psrs_core::ExprKind::Let { bindings, .. } = &main.value.kind else {
        panic!("main should retain its record binding in Core");
    };
    let record = bindings
        .iter()
        .find(|binding| binding.binder.name == "record")
        .map(|binding| &binding.value)
        .expect("Core should retain the record value");
    let psrs_core::ExprKind::Record { fields } = &record.kind else {
        panic!("binding should be a record expression");
    };
    assert_eq!(
        fields
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["z", "a"]
    );
    let psrs_core::Type::Record(record_type_fields) = &core.types[record.ty.0 as usize] else {
        panic!("record binding type should be a closed record");
    };
    assert_eq!(
        record_type_fields
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"z\na\n");
}

#[test]
fn runs_a_number_record_field_through_a_gc_struct() {
    let source = "module Main where\nuse :: Number -> Int\nuse x = 42\nmain = use ({ answer: 1.5 }.answer)\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a Number record field");
    assert!(artifact.wasm.len() > 8);
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_record_update_through_a_gc_struct() {
    let source = "module Main where\nmain = { ignored: 10, answer: 1 } { answer = 42 }.answer\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a record update");
    assert!(artifact.wat.contains("struct.new"));
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_nested_record_update_through_a_gc_struct() {
    let source = "module Main where\nmain = { address: { city: 1, zip: 2 }, ignored: 0 } { address { city = 42 } }.address.city\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a nested record update");
    assert!(artifact.wat.contains("struct.new"));
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_record_pattern_in_a_case() {
    let source = "module Main where\nmain = case { ignored: 10, answer: 42 } of\n  { ignored: _, answer: value } -> value\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a record pattern");
    assert!(artifact.wasm.len() > 8);
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_record_pattern_in_a_function_parameter() {
    let source = "module Main where\nanswer { ignored: ignored, answer: value } = value + ignored\nmain = answer { ignored: 10, answer: 32 }\n";
    let artifact =
        compile_source("Main.purs", source).expect("lowering a record pattern function parameter");
    assert!(artifact.wat.contains("struct.get"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_nested_constructor_pattern_in_a_record_field() {
    let source = "module Main where\ndata Inner = First Int | Second Int\nunpack value = case value of\n  { payload: First number } -> number + 0\n  _ -> 0\nmain = unpack { payload: First 42 }\n";
    let artifact =
        compile_source("Main.purs", source).expect("lowering a nested constructor record pattern");
    assert!(artifact.wat.contains("struct.get"));
    assert!(artifact.wat.contains("i32.eq"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn falls_back_when_a_nested_record_pattern_constructor_does_not_match() {
    let source = "module Main where\ndata Inner = First Int | Second Int\nunpack value = case value of\n  { payload: First number } -> number + 0\n  _ -> 7\nmain = unpack { payload: Second 10 }\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn runs_a_nested_record_pattern_in_a_record_field() {
    let source = "module Main where\nunpack value = case value of\n  { payload: { answer: number } } -> number\n  _ -> 0\nmain = unpack { payload: { answer: 42 } }\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn rejects_a_record_update_with_an_unknown_field() {
    let source = "module Main where\nmain = { answer: 1 } { missing = 42 }.answer\n";
    let errors = compile_source("Main.purs", source).expect_err("unknown record field");
    assert!(errors.iter().any(|error| {
        error.stage == "P5 typecheck" && error.message.contains("record has no field `missing`")
    }));
}

#[test]
fn rejects_a_record_update_with_duplicate_fields() {
    let source = "module Main where\nmain = { answer: 1 } { answer = 2, answer = 3 }.answer\n";
    let errors = compile_source("Main.purs", source).expect_err("duplicate record field");
    assert!(errors.iter().any(|error| {
        error.stage == "P5 typecheck"
            && error
                .message
                .contains("record label `answer` occurs more than once")
    }));
}

#[test]
fn rejects_a_record_pattern_with_an_unknown_field() {
    let source = "module Main where\nmain = case { answer: 1 } of\n  { missing: value } -> value\n";
    let errors = compile_source("Main.purs", source).expect_err("unknown record pattern field");
    assert!(errors.iter().any(|error| {
        error.stage == "P5 typecheck" && error.message.contains("record has no field `missing`")
    }));
}

#[test]
fn typechecks_an_open_record_row_by_label() {
    let source = "\
module Main where
getX :: forall r. { x :: Int | r } -> Int
getX record = record.x
setX :: forall r. { x :: Int | r } -> String -> { x :: String | r }
setX record value = record { x = value }
same :: forall r. { y :: Boolean, x :: Int | r } -> Int
same record = getX record
swapped :: forall s. { x :: Int, y :: Boolean | s } -> Int
swapped record = same record
main = swapped { y: true, x: 1 }
";
    check_source("Main.purs", source).expect("open rows should type check");
    let errors = compile_source("Main.purs", source).expect_err("open rows have no runtime layout");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("open record rows have no runtime layout")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_missing_open_record_field_and_a_duplicate_label() {
    let missing = "\
module Main where
getX :: forall r. { x :: Int | r } -> Int
getX record = record.x
main = getX { y: true }
";
    let errors = check_source("Main.purs", missing).expect_err("missing label");
    assert!(errors.iter().any(|error| {
        error.stage == "P5 typecheck" && error.message.contains("record has no field `x`")
    }));
    let hidden = "\
module Main where
getY :: forall r. { x :: Int | r } -> Int
getY record = record.y
main = 0
";
    let errors = check_source("Main.purs", hidden).expect_err("rigid tail");
    assert!(errors.iter().any(|error| {
        error.stage == "P5 typecheck" && error.message.contains("record has no field `y`")
    }));
    let duplicate = "\
module Main where
bad :: { x :: Int, x :: Boolean } -> Int
bad record = record.x
main = bad { x: 1 }
";
    let errors = check_source("Main.purs", duplicate).expect_err("duplicate label");
    assert!(errors.iter().any(|error| {
        error.stage == "P5 typecheck"
            && error
                .message
                .contains("record label `x` occurs more than once")
    }));
}

#[test]
fn rejects_open_record_patterns() {
    let source =
        "module Main where\nmain = case { answer: 1 } of\n  { answer: value ..rest } -> value\n";
    let errors = compile_source("Main.purs", source).expect_err("open record pattern");
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("open record patterns are not supported yet")
    }));
}

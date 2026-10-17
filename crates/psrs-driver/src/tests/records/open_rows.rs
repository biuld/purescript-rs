use super::*;

#[test]
fn a_generic_record_accessor_uses_labels_across_distinct_closed_rows() {
    let source = r#"module Main where
readValue :: forall r. { value :: Int | r } -> Int
readValue record = record.value
main :: Int
main = if intEq (readValue { a: false, value: 19 }) 19
       then if intEq (readValue { value: 23, z: "λ😀" }) 23 then 42 else 1
       else 2
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn a_recursive_open_row_function_keeps_the_complete_record() {
    let source = r#"module Main where
readValue :: forall r. Int -> { value :: Int | r } -> Int
readValue count record = if intEq count 0 then record.value
                         else readValue (intSub count 1) record
main :: Int
main = readValue 5 { a: "λ😀", value: 42, z: false }
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn an_open_row_result_recovers_the_callers_complete_closed_product() {
    let source = r#"module Main where
keep :: forall r. { value :: Int | r } -> { value :: Int | r }
keep record = record
main :: Int
main = let result = keep { a: 19, value: 42, z: "λ😀" }
       in if intEq result.a 19 then result.value else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn an_adt_erased_field_retains_a_records_open_row_storage() {
    let source = r#"module Main where
data Box a = Box a
readBox :: forall r. Box { value :: Int | r } -> Int
readBox (Box record) = record.value
main :: Int
main = readBox (Box { a: "λ😀", value: 42 })
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn an_array_erased_slot_retains_a_records_open_row_storage() {
    let source = r#"module Main where
readArray :: forall r. Array { value :: Int | r } -> Int
readArray values = (arrayIndex values 0).value
main :: Int
main = readArray [{ a: "λ😀", value: 42 }]
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn nested_open_rows_keep_independent_tails() {
    let source = r#"module Main where
readNested :: forall inner outer. { nested :: { value :: Int | inner } | outer } -> Int
readNested record = record.nested.value
main :: Int
main = readNested { a: false, nested: { a: "λ😀", value: 42 } }
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn a_callable_field_recovers_through_the_record_payload_owner() {
    let source = r#"module Main where
applyRecord :: forall r. { callback :: Int -> Int | r } -> Int
applyRecord record = record.callback 42
main :: Int
main = applyRecord { a: "λ😀", callback: \value -> value }
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn a_callable_result_converts_its_closed_record_into_open_storage() {
    let source = r#"module Main where
consume :: forall r. (Int -> { value :: Int | r }) -> Int
consume make = (make 0).value
produce :: Int -> { label :: String, value :: Int }
produce _ = { label: "λ😀", value: 42 }
main :: Int
main = consume produce
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}


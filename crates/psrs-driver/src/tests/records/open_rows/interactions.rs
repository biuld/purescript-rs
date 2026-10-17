use super::*;

// OR-04/OR-05 combination coverage: interactions the earlier open-row tests do
// not exercise. All realize through the labelled record store without any
// specialization.

#[test]
fn an_erased_polymorphic_transport_keeps_the_record_metadata() {
    // The design invariant for bare erased slots: passing a record through
    // `forall a. a -> a` must not discard the label/value sequence, so the
    // value still opens at a later open-row use.
    let source = r#"module Main where
read :: forall r. { value :: Int | r } -> Int
read record = case record of { value: v } -> v
wrap :: forall a. a -> a
wrap x = x
main :: Int
main = if intEq (read (wrap { z: true, value: 42 })) 42 then 42 else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn a_record_pattern_inside_a_constructor_pattern_selects_open_row_labels() {
    let source = r#"module Main where
data Maybe a = Nothing | Just a
readJust :: forall r. Maybe { value :: Int | r } -> Int
readJust (Just { value }) = value
readJust Nothing = 0
other :: forall s. Maybe { kept :: Boolean | s } -> Boolean
other (Just { kept }) = kept
other Nothing = false
main :: Int
main = if intEq (readJust (Just { z: true, value: 40 })) 40
         then if other (Just { kept: true, elsewhere: 9 }) then 42 else 2
         else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn two_record_pattern_columns_select_their_own_labels() {
    let source = r#"module Main where
combine :: forall r s. { value :: Int | r } -> { flag :: Boolean | s } -> Int
combine { value } { flag } = if flag then value else 2
main :: Int
main = intAdd (combine { a: "x", value: 40 } { b: 9, flag: true })
              (combine { value: 20 } { flag: false })
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn a_wildcard_field_label_does_not_bind_or_consume_the_tail() {
    let source = r#"module Main where
read :: forall r. { ignored :: Boolean, value :: Int | r } -> Int
read record = case record of { ignored: _, value: v } -> v
main :: Int
main = if intEq (read { ignored: true, z: "x", value: 42 }) 42 then 42 else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn a_callable_field_extracted_by_a_pattern_applies_immediately() {
    let source = r#"module Main where
use :: forall r. { f :: Int -> Int | r } -> Int
use record = case record of { f: g } -> g 3
main :: Int
main = if intEq (use { extra: [1], f: \x -> intAdd x 39 }) 42 then 42 else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn a_guard_after_a_record_pattern_reads_the_bound_label() {
    let source = r#"module Main where
read :: forall r. { value :: Int | r } -> Int
read record = case record of
  { value: v } | intEq v 42 -> 42
  _ -> 2
main :: Int
main = intAdd (read { z: true, value: 42 }) (read { value: 1 })
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(44), "{output:?}");
}

#[test]
fn one_recovered_erased_record_serves_two_use_types() {
    // A value recovered from an erased ADT slot keeps one authoritative label
    // store and is reused at two different open-row use types without a
    // second pack.
    let source = r#"module Main where
import Partial.Unsafe (unsafePartial)
data Maybe a = Nothing | Just a
fromJust :: forall a. Partial => Maybe a -> a
fromJust (Just x) = x
read :: forall r. { value :: Int | r } -> Int
read record = case record of { value: v } -> v
reader :: forall s. { kept :: Boolean | s } -> Boolean
reader record = case record of { kept: k } -> k
continue :: { kept :: Boolean, value :: Int } -> Int
continue recovered = if reader recovered
  then read { kept: 0, value: recovered.value }
  else 1
main :: Int
main = unsafePartial (continue (fromJust (Just { kept: true, value: 20 })))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(20), "{output:?}");
}

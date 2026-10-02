//! Value-sensitive execution checks for scalar and length pattern decisions.

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const PAIR_FIRST_WINS: &str = r#"module Main where
data Pair = Pair Int String
select :: Pair -> Int
select value | Pair x x <- value = x
select _ = 0
makePair :: Int -> Pair
makePair n = if n == 0 then Pair 41 "other" else makePair (n - 1)
main = select (makePair 1)
"#;

// These mirror the upstream pattern shapes while making `main` observe the
// selected field. The upstream corpus files only print "Done".
const MULTIFIELD_CONSTRUCTOR_1185: &str = r#"module Main where
data Person = Person String Boolean
getName :: Person -> String
getName p = case p of
  Person name true -> name
  _ -> "Unknown"
main = case getName (Person "John Smith" true) of
  "John Smith" -> 85
  _ -> 0
"#;

const NESTED_NAMED_RECORD_2049: &str = r#"module Main where
import Prelude
data List a = Cons a (List a) | Nil
infixr 6 Cons as :
f :: List { x :: Int, y :: Int } -> Int
f (r@{ x } : _) = x + r.y
f _ = 0
main = f ({ x: 31, y: 53 } : Nil)
"#;

const NESTED_RECORD: &str = r#"module Main where
type Input = { outer :: { enabled :: Boolean, value :: Int }, items :: Array Int }
choose :: Input -> Int
choose input = case input of
  whole@{ outer: { value: inner, enabled: true }, items: [first, 0] } -> whole.outer.value + inner
  _ -> 0
makeInput :: Int -> Input
makeInput n = if n == 0
  then { outer: { value: 42, enabled: true }, items: [7, 0] }
  else makeInput (n - 1)
main = choose (makeInput 1)
"#;

const ARRAY_FALLTHROUGH: &str = r#"module Main where
choose :: Array Int -> Int
choose values = case values of
  [7, 9] -> 42
  [7, other] -> other
  _ -> 0
makeShort :: Int -> Array Int
makeShort n = if n == 0 then [7] else makeShort (n - 1)
makeLong :: Int -> Array Int
makeLong n = if n == 0 then [7, 8] else makeLong (n - 1)
main = choose (makeShort 1) + choose (makeLong 1)
"#;

const UTF8_STRING: &str = r#"module Main where
choose :: String -> Int
choose value = case value of
  "" -> 3
  "🙃" -> 99
  "🙂" -> 42
  "é" -> 1
  _ -> 0
makeText :: Int -> String
makeText n = if n == 0 then bytesToString (stringToBytes "🙂") else makeText (n - 1)
makeEmpty n = if n == 0 then bytesToString (stringToBytes "") else makeEmpty (n - 1)
main = choose (makeText 1) + choose (makeEmpty 1)
"#;

const NUMBER_CHAR: &str = r#"module Main where
chooseNumber :: Number -> Int
chooseNumber value = case value of
  0.0 -> 20
  -0.0 -> 30
  _ -> 0
makeNumber :: Int -> Number
makeNumber n = if n == 0 then numberNeg 0.0 else makeNumber (n - 1)
chooseChar :: Char -> Int
chooseChar value = case value of
  '😃' -> 1
  '😀' -> 42
  _ -> 0
makeChar :: Int -> Char
makeChar n = if n == 0 then '😀' else makeChar (n - 1)
main = chooseNumber (makeNumber 1) + chooseChar (makeChar 1)
"#;

fn wasmtime_is_available() -> bool {
    let version = Command::new("wasmtime").arg("--version").output();
    let required = std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1");
    let usable = matches!(&version, Ok(output) if output.status.success());
    if !usable && required {
        panic!("PSRS_REQUIRE_WASMTIME=1 but Wasmtime is not usable: {version:?}");
    }
    usable
}

fn assert_runs(name: &str, source: &str, expected: i32, cc_markers: &[&str]) {
    let compilation = psrs_driver::compile_source_with_dumps(name, source)
        .unwrap_or_else(|errors| panic!("{name} did not compile: {errors:?}"));
    for marker in cc_markers {
        assert!(
            compilation.dumps.cc.contains(marker),
            "{name} should retain the runtime decision operation {marker} after optimization"
        );
    }

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "psrs-pattern-matrix-{}-{name}-{id}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, compilation.artifact.wasm).expect("writing pattern component");
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("running pattern component");
    let _ = std::fs::remove_file(path);
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{name} produced the wrong value: {output:?}"
    );
}

fn require_wasmtime() -> bool {
    if wasmtime_is_available() {
        true
    } else {
        eprintln!("skipping pattern matrix execution: Wasmtime is unavailable");
        false
    }
}

#[test]
fn pattern_guard_duplicate_binding_selects_the_first_constructor_field() {
    if !require_wasmtime() {
        return;
    }
    assert_runs("pair_first_wins", PAIR_FIRST_WINS, 41, &["ProductGet"]);
}

#[test]
fn upstream_1185_multifield_constructor_and_boolean_pattern_select_the_name() {
    if !require_wasmtime() {
        return;
    }
    assert_runs(
        "multifield_constructor_1185",
        MULTIFIELD_CONSTRUCTOR_1185,
        85,
        &["StringEq"],
    );
}

#[test]
fn upstream_2049_nested_named_record_pattern_returns_the_selected_record_value() {
    if !require_wasmtime() {
        return;
    }
    assert_runs(
        "nested_named_record_2049",
        NESTED_NAMED_RECORD_2049,
        84,
        &["ProductGet"],
    );
}

#[test]
fn nested_record_and_named_alias_patterns_return_the_selected_field() {
    if !require_wasmtime() {
        return;
    }
    assert_runs(
        "nested_record",
        NESTED_RECORD,
        84,
        &["ArrayLen", "ProductGet"],
    );
}

#[test]
fn short_arrays_and_failed_elements_fall_through_without_escaping_length_guard() {
    if !require_wasmtime() {
        return;
    }
    assert_runs(
        "array_fallthrough",
        ARRAY_FALLTHROUGH,
        8,
        &["ArrayLen", "ArrayGet"],
    );
}

#[test]
fn string_patterns_compare_canonical_utf8_bytes_for_non_ascii_and_astral_scalars() {
    if !require_wasmtime() {
        return;
    }
    assert_runs("utf8_string", UTF8_STRING, 45, &["StringEq"]);
}

#[test]
fn number_zero_and_astral_char_patterns_compare_their_source_values() {
    if !require_wasmtime() {
        return;
    }
    assert_runs("number_char", NUMBER_CHAR, 62, &["NumberEq", "CharEq"]);
}

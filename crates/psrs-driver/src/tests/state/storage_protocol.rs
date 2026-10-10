use super::*;

fn source(item: &str, initial: &str, replacement: &str, predicate: &str) -> String {
    format!(
        r#"module Main where
import Prim.State (State, RealWorld)
foreign import "psrs:intrinsic#intEq" eqInt :: Int -> Int -> Boolean
foreign import "psrs:intrinsic#numberEq" eqNumber :: Number -> Number -> Boolean
foreign import "psrs:intrinsic#booleanEq" eqBoolean :: Boolean -> Boolean -> Boolean
foreign import "psrs:intrinsic#stringToBytes" bytes :: String -> Array Int
foreign import "psrs:intrinsic#arrayLength" length :: forall a. Array a -> Int
foreign import "psrs:intrinsic#arrayIndex" index :: forall a. Array a -> Int -> a
foreign import "psrs:intrinsic#intAdd" addInt :: Int -> Int -> Int
type Item = {item}
type Step s a = {{ state :: State s, value :: a }}
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
foreign import "psrs:runtime-storage#array_fill" fill :: Int -> Item -> State RealWorld -> Step RealWorld (Array Item)
foreign import "psrs:runtime-storage#array_write" write :: Array Item -> Int -> Item -> State RealWorld -> Step RealWorld Unit
foreign import "psrs:runtime-storage#array_read" read :: Array Item -> Int -> State RealWorld -> Step RealWorld Item
action :: State RealWorld -> Step RealWorld Int
action state0 = case fill 1 ({initial}) state0 of
  {{ state: state1, value: items }} ->
    let alias = items in case write items 0 ({replacement}) state1 of
      {{ state: state2, value: ignored }} -> case read alias 0 state2 of
        {{ state: state3, value: item }} -> {{ state: state3, value: if {predicate} then 42 else 1 }}
main :: Int
main = execute action
"#
    )
}

#[test]
fn monomorphic_storage_recovers_scalars_records_and_callbacks_without_copying_the_array() {
    for (item, initial, replacement, predicate) in [
        ("Int", "40", "2147483647", "eqInt item 2147483647"),
        ("Number", "40.5", "42.25", "eqNumber item 42.25"),
        ("Boolean", "true", "false", "eqBoolean item false"),
        (
            "String",
            "\"before\"",
            "\"after\"",
            "if eqInt (length (bytes item)) 5 then eqInt (index (bytes item) 0) 97 else false",
        ),
        (
            "{ count :: Int }",
            "{ count: 40 }",
            "{ count: 42 }",
            "eqInt item.count 42",
        ),
        (
            "(Int -> Int)",
            "\\n -> n",
            "\\n -> addInt n 1",
            "eqInt (item 41) 42",
        ),
    ] {
        let source = source(item, initial, replacement, predicate);
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert_eq!(output.status.code(), Some(42), "{item}: {output:?}");
    }
}

#[test]
fn monomorphic_partial_read_keeps_its_checked_source_result() {
    let source = source("Int", "40", "42", "eqInt item 42").replace(
        "case read alias 0 state2 of",
        "let later = read alias 0 in case later state2 of",
    );
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn monomorphic_nested_storage_preserves_element_aliases_across_slots() {
    let source = state_flow_source(r#"let shared = [40] in case fillNested 2 shared state0 of
  { state: state1, value: outer } -> case readNested outer 0 state1 of
    { state: state2, value: first } -> case writeInt first 0 42 state2 of
      { state: state3, value: ignored } -> case readNested outer 1 state3 of
        { state: state4, value: second } -> readInt second 0 state4"#).replace("action ::", r#"foreign import "psrs:runtime-storage#array_fill" fillNested :: Int -> Array Int -> State RealWorld -> Step RealWorld (Array (Array Int))
foreign import "psrs:runtime-storage#array_read" readNested :: Array (Array Int) -> Int -> State RealWorld -> Step RealWorld (Array Int)
foreign import "psrs:runtime-storage#array_write" writeInt :: Array Int -> Int -> Int -> State RealWorld -> Step RealWorld Unit
foreign import "psrs:runtime-storage#array_read" readInt :: Array Int -> Int -> State RealWorld -> Step RealWorld Int
action ::"#);
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn runtime_storage_protocol_is_selected_after_source_checking_and_before_mir_layout() {
    use psrs_backend::cc::{self, AssignmentKind, ValueConversion};
    let source = source("Int", "40", "42", "eqInt item 42");
    let core = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let input = cc::lower_module(core).unwrap();
    let reader = input
        .externals
        .runtime
        .iter()
        .find(|binding| binding.function == "array_read")
        .unwrap();
    let signature = input
        .cc
        .externals
        .iter()
        .find(|external| external.symbol == reader.symbol)
        .unwrap()
        .signature
        .as_ref()
        .unwrap();
    let projection = cc::state::StateCallProjection::checked(signature, &input.cc.representations)
        .unwrap()
        .unwrap();
    assert_eq!(
        projection.payload,
        cc::ValueShape::Reference(cc::Reference {
            nullable: false,
            heap: cc::RefShape::Erased
        })
    );
    assert!(input.cc.functions.iter().flat_map(|function| &function.assignments).any(|assignment|
        matches!(&assignment.kind, AssignmentKind::AggregateConvert { conversion, .. }
            if matches!(conversion.plan, ValueConversion::UnboxScalar { destination: cc::ValueShape::Integer, .. }))));
    cc::state::check(&input.cc).unwrap();
}

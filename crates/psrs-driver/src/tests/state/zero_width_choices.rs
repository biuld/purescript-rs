use super::*;

fn source(flag: i32, choice: &str) -> String {
    SOURCE.replace("[1]", &format!("[{flag}]"))
        .replace("if eqInt flag 1 then\n        case write values 0 42 initial of\n          { state: next, value: ignored } -> next\n        else case write values 0 43 initial of\n          { state: next, value: ignored } -> next", choice)
}

const SOURCE: &str = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
foreign import "psrs:runtime-storage#array_write" write :: Array Int -> Int -> Int -> State RealWorld -> Step RealWorld Unit
foreign import "psrs:runtime-storage#array_read" read :: Array Int -> Int -> State RealWorld -> Step RealWorld Int

foreign import "psrs:intrinsic#intEq" eqInt :: Int -> Int -> Boolean
action :: State RealWorld -> Step RealWorld Int
action state0 = case read [1] 0 state0 of
 { state: initial, value: flag } ->
  let values = [0]
      state1 = if eqInt flag 1 then
        case write values 0 42 initial of
          { state: next, value: ignored } -> next
        else case write values 0 43 initial of
          { state: next, value: ignored } -> next
  in read values 0 state1
main :: Int
main = execute action
"#;

const BINARY: &str = "if eqInt flag 1 then\n        case write values 0 42 initial of\n          { state: next, value: ignored } -> next\n        else case write values 0 43 initial of\n          { state: next, value: ignored } -> next";
const SWITCH: &str = "case flag of\n        1 -> case write values 0 42 initial of\n          { state: next, value: ignored } -> next\n        2 -> case write values 0 43 initial of\n          { state: next, value: ignored } -> next\n        _ -> case write values 0 44 initial of\n          { state: next, value: ignored } -> next";

#[test]
fn zero_width_choice_results_keep_selected_mutation_and_successor() {
    for (choice, flag, expected) in [
        (BINARY, 1, 42),
        (BINARY, 2, 43),
        (SWITCH, 1, 42),
        (SWITCH, 2, 43),
        (SWITCH, 3, 44),
    ] {
        let source = source(flag, choice);
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert_eq!(output.status.code(), Some(expected), "{source}\n{output:?}");
    }
}

#[test]
fn zero_width_choice_preserves_selected_nonreturning_arms() {
    for (flag, expected) in [(1, 134), (2, 43)] {
        let source = source(flag, BINARY)
            .replace(
                "action ::",
                "foreign import \"psrs:runtime-storage#trap\" abort :: State RealWorld -> Step RealWorld Unit\naction ::",
            )
            .replace("case write values 0 42 initial of", "case abort initial of");
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert_eq!(output.status.code(), Some(expected), "{source}\n{output:?}");
        if flag == 1 {
            assert!(String::from_utf8_lossy(&output.stderr).contains("unreachable"));
        }
    }
}

#[test]
fn zero_width_choice_cannot_replay_its_observable_predecessor() {
    for choice in [BINARY, SWITCH] {
        let source =
            source(1, choice).replace("in read values 0 state1", "in read values 0 initial");
        let errors = lower_program_to_core(&[("Main.purs", &source)]).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage.starts_with("P7")),
            "{errors:?}"
        );
    }
}

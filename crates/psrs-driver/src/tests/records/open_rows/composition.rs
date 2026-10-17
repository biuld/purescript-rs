use super::*;

#[test]
fn local_open_row_closures_execute_without_core_specialization() {
    let source = r#"module Main where
main :: Int
main =
  let captured = 2
      select :: forall r. { value :: Int | r } -> Int
      select { value: x } = intAdd x captured
      first = select { value: 20, left: true }
      second = select { value: 18, right: [1,2] }
  in intAdd first second
"#;
    let Some(()) = wasmtime_available() else {
        return;
    };
    let core = crate::program::lower_program_to_core(&[("Main.purs", source)])
        .expect("source checks and lowers to unoptimized Core");
    let input = psrs_backend::cc::lower_module(core).expect("generic local row lowers directly");
    let readers = input
        .cc
        .functions
        .iter()
        .filter(|function| {
            function.assignments.iter().any(|assignment| {
                matches!(
                    assignment.kind,
                    psrs_backend::cc::AssignmentKind::Record {
                        operation: psrs_backend::cc::RecordOperation::Find { .. },
                        ..
                    }
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(readers.len(), 1, "one generic row reader serves both uses");
    let artifact = psrs_backend::compile_cc(input).expect("unoptimized generic CC emits");
    let id = WASM_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("psrs-or07-{}-{id}.wasm", std::process::id()));
    std::fs::write(&path, artifact.wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn imported_recursive_updates_and_nested_callable_payloads_keep_unknown_tails() {
    let library = r#"module Rows where
data Box a = Box a
update :: forall a r. { value :: a | r } -> { value :: Int | r }
update record = record { value = 21 }
repeat :: forall r. Int -> { value :: Int | r } -> { value :: Int | r }
repeat n record = if intEq n 0 then record
                  else repeat (intSub n 1) (record { value = intAdd record.value 1 })
read :: forall r. Box (Array { value :: Int | r }) -> Int
read (Box values) = case arrayIndex values 0 of
  { value } -> value
"#;
    let main = r#"module Main where
import Rows
main :: Int
main = let original = { a: "kept", value: false, z: { callback: \x -> intAdd x 1 } }
           result = repeat 20 (update original)
       in if original.value then 1
          else case result.a of
            "kept" -> result.z.callback (read (Box [result]))
            _ -> 2
"#;
    let Some(output) = run_program_with_wasmtime(&[("Rows.purs", library), ("Main.purs", main)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn a_local_row_polymorphic_reader_is_passed_at_an_open_use_type() {
    let source = r#"module Main where
apply :: forall r. ({ value :: Int | r } -> Int) -> { value :: Int | r } -> Int
apply f record = f record
read :: forall r. Int -> { value :: Int | r } -> Int
read captured record =
  let select :: forall s. { value :: Int | s } -> Int
      select { value } = intAdd value captured
  in apply select record
main :: Int
main = intAdd (read 1 { a: false, value: 19 }) (read 2 { value: 20, z: "kept" })
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn captured_recursive_local_row_updates_preserve_each_instantiations_tail() {
    let source = r#"module Main where
increment :: forall r. Int -> { value :: Int | r } -> { value :: Int | r }
increment amount record =
  let step = 2
      go :: forall s. Int -> { value :: Int | s } -> { value :: Int | s }
      go n current = if intEq n 0 then current
                     else go (intSub n 1) (current { value = intAdd current.value step })
  in go amount record
main :: Int
main = let first = increment 10 { a: "kept", value: 0 }
           second = increment 10 { value: 2, z: [false] }
       in case first.a of
            "kept" -> if arrayIndex second.z 0 then 1 else intAdd first.value second.value
            _ -> 2
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

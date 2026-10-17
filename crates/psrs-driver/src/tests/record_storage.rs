use super::*;

const CARRIER: &str = r#"module Carrier (Cell, pack, unpack) where
foreign import "psrs:intrinsic#__psrs_unsafe_coerce" unsafeCoerce :: forall a b. a -> b
data Cell
pack :: forall a. a -> Cell
pack = unsafeCoerce
unpack :: forall a. Cell -> a
unpack = unsafeCoerce
"#;

const PARAMETERIZED_CARRIER: &str = r#"module Carrier (Cell, pack, unpack) where
foreign import "psrs:intrinsic#__psrs_unsafe_coerce" unsafeCoerce :: forall a b. a -> b
data Cell a
pack :: forall a. a -> Cell Int
pack = unsafeCoerce
unpack :: forall a. Cell Int -> a
unpack = unsafeCoerce
"#;

const PARAMETERIZED_STORAGE: &str = r#"module Storage where
import Carrier (Cell)
foreign import "psrs:record-storage#length" size :: forall r. Record r -> Int
foreign import "psrs:record-storage#payload_at" payloadAt :: forall r. Int -> Record r -> Cell Int
foreign import "psrs:record-storage#build" build :: forall r. Array String -> Array (Cell Int) -> Record r
"#;

const STORAGE: &str = r#"module Storage where
import Carrier (Cell)
foreign import "psrs:record-storage#length" size :: forall r. Record r -> Int
foreign import "psrs:record-storage#key_at" keyAt :: forall r. Int -> Record r -> String
foreign import "psrs:record-storage#payload_at" payloadAt :: forall r. Int -> Record r -> Cell
foreign import "psrs:record-storage#build" build :: forall r. Array String -> Array Cell -> Record r
foreign import "psrs:runtime-storage#array_write" write :: forall a. Array a -> Int -> a -> Unit
"#;

#[test]
fn record_primitives_accept_an_imported_applied_constructorless_slot() {
    let main = r#"module Main where
import Carrier (pack, unpack)
import Storage
foreign import "psrs:intrinsic#intEq" eq :: Int -> Int -> Boolean
main :: Int
main =
  let record = build ["value"] [pack 42]
  in if eq (size record) 1
     then unpack (payloadAt 0 record) :: Int
     else 0
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Carrier.purs", PARAMETERIZED_CARRIER),
        ("Storage.purs", PARAMETERIZED_STORAGE),
        ("Main.purs", main),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn record_primitives_keep_imported_slots_and_snapshot_build_inputs() {
    let main = r#"module Main where
import Carrier (pack, unpack)
import Storage
foreign import "psrs:intrinsic#intEq" eq :: Int -> Int -> Boolean
invoke :: forall r. (Record r -> Int) -> Record r -> Int
invoke f value = f value
main :: Int
main =
  let keys = ["a", "nested", "value"]
      cells = [pack 17, pack { inner: 42 }, pack (\x -> x)]
      record = build keys cells
      changedKeys = write keys 1 "corrupt"
      changedCells = write cells 1 (pack { inner: 1 })
      nested = unpack (payloadAt 1 record) :: { inner :: Int }
      function = unpack (payloadAt 2 record) :: Int -> Int
  in if eq (invoke size record) 3
     then if eq (arrayLength (stringToBytes (keyAt 1 record))) 6
          then function nested.inner
          else 2
     else 3
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Carrier.purs", CARRIER),
        ("Storage.purs", STORAGE),
        ("Main.purs", main),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn record_primitives_adapt_closed_records_and_order_unicode_labels() {
    let main = r#"module Main where
import Carrier (Cell, pack, unpack)
import Storage
foreign import "psrs:intrinsic#intEq" eq :: Int -> Int -> Boolean
foreign import "psrs:record-storage#length" closedSize :: { value :: Int } -> Int
foreign import "psrs:record-storage#build" buildClosed :: Array String -> Array Cell -> { value :: Int }
main :: Int
main =
  let closed = buildClosed ["value"] [pack 42]
      unicode = build ["", "a", "a😀", "b", "😀"]
        [pack 1, pack 2, pack 3, pack 4, pack 5]
      empty = build [] []
  in if eq (intAdd closed.value (closedSize closed)) 43
     then if eq (size unicode) 5
          then if eq (arrayLength (stringToBytes (keyAt 2 unicode))) 5
               then if eq (arrayLength (stringToBytes (keyAt 4 unicode))) 4
                    then if eq (unpack (payloadAt 2 unicode) :: Int) 3
                         then if eq (size empty) 0 then 42 else 1
                         else 1
                    else 1
               else 1
          else 1
     else 1
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Carrier.purs", CARRIER),
        ("Storage.purs", STORAGE),
        ("Main.purs", main),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn record_build_rejects_bad_parallel_arrays_and_label_order() {
    let main = [
        "size (build [\"a\", \"a\"] [pack 1, pack 2])",
        "size (build [\"b\", \"a\"] [pack 1, pack 2])",
        "size (build [\"a\"] [])",
    ];
    for body in main {
        let source = format!(
            "module Main where\nimport Carrier (pack)\nimport Storage\nmain :: Int\nmain = {body}\n"
        );
        let Some(output) = run_program_with_wasmtime(&[
            ("Carrier.purs", CARRIER),
            ("Storage.purs", STORAGE),
            ("Main.purs", &source),
        ]) else {
            return;
        };
        assert!(!output.status.success(), "{body}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unreachable"),
            "{body}: {output:?}"
        );
    }
}

#[test]
fn record_reads_trap_on_negative_and_out_of_range_indices() {
    let main = [
        "arrayLength (stringToBytes (keyAt (intNeg 1) (build [\"a\"] [pack 42])))",
        "arrayLength (stringToBytes (keyAt 1 (build [\"a\"] [pack 42])))",
        "if eq (unpack (payloadAt 1 (build [\"a\"] [pack 42])) :: Int) 42 then 1 else 0",
    ];
    for body in main {
        let source = format!(
            "module Main where\nimport Carrier (pack, unpack)\nimport Storage\nforeign import \"psrs:intrinsic#intEq\" eq :: Int -> Int -> Boolean\nmain :: Int\nmain = {body}\n"
        );
        let Some(output) = run_program_with_wasmtime(&[
            ("Carrier.purs", CARRIER),
            ("Storage.purs", STORAGE),
            ("Main.purs", &source),
        ]) else {
            return;
        };
        assert!(!output.status.success(), "{body}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unreachable"),
            "{body}: {output:?}"
        );
    }
}

#[test]
fn record_storage_validates_unused_source_contracts_before_erasure() {
    for (module, export, expected) in [
        (
            "module Storage where\nforeign import \"psrs:record-storage#length\" op :: forall r. Record r -> Number\n",
            "length",
            "record primitive signature expects Int",
        ),
        (
            "module Storage where\nforeign import \"psrs:record-storage#payload_at\" op :: forall r a. Int -> Record r -> a\n",
            "payload_at",
            "record primitive Slot must be a declared constructorless ordinary data type",
        ),
        (
            "module Storage where\ndata Cell = Cell\nforeign import \"psrs:record-storage#payload_at\" op :: forall r. Int -> Record r -> Cell\n",
            "payload_at",
            "record primitive Slot must be a declared constructorless ordinary data type",
        ),
        (
            "module Storage where\nimport Carrier (Cell)\nforeign import \"psrs:record-storage#build\" op :: Array Int -> Array Cell -> Record ()\n",
            "build",
            "record primitive signature expects Array String",
        ),
    ] {
        let main = "module Main where\nimport Storage\nmain :: Int\nmain = 42\n";
        let errors = compile_program_sources(&[
            ("Carrier.purs", CARRIER),
            ("Storage.purs", module),
            ("Main.purs", main),
        ])
        .expect_err("unused invalid record providers must reject during source linking");
        assert!(
            errors.iter().any(|error| {
                error.diagnostic.stage == "P8 runtime linking"
                    && error.diagnostic.message.contains(expected)
            }),
            "{export}: {errors:?}"
        );
    }
}

use super::*;

#[test]
fn writes_and_reads_a_file_through_preopens_when_wasmtime_is_available() {
    let source = r#"module Main where
import Effect.Unsafe (unsafePerformEffect)
import Prelude
import Data.Either (Either(..))
import WASI.Console (log)
import WASI.FileSystem
main =
  let dirs = unsafePerformEffect preopens in
  let dir = arrayIndex dirs 0 in
  let created = unsafePerformEffect (openWrite (dir._1) "roundtrip.txt") in
  case created of
    Left _ -> 1
    Right file ->
      let wrote = unsafePerformEffect (writeString file "roundtrip") in
      let closed = unsafePerformEffect (dropDescriptor file) in
      let reopened = unsafePerformEffect (openRead (dir._1) "roundtrip.txt") in
      case reopened of
        Left _ -> 2
        Right reader ->
          let contents = unsafePerformEffect (withDescriptor reader (\openReader -> readString openReader 64)) in
          case contents of
            Left _ -> 3
            Right text -> let ignored = unsafePerformEffect (log text) in 0
"#;
    let directory = std::env::temp_dir().join(format!(
        "psrs-fs-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let Some(output) = run_wasmtime_with_dirs(source, &[], None, &[(directory.clone(), "/data")])
    else {
        eprintln!("skipping: wasmtime is not installed");
        let _ = std::fs::remove_dir_all(&directory);
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"roundtrip\n", "{output:?}");
    let written = std::fs::read(directory.join("roundtrip.txt")).expect("the guest wrote the file");
    assert_eq!(written, b"roundtrip");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn reports_a_unit_success_result_as_left_when_wasmtime_is_available() {
    // DEC-13: `result<_, error-code>` maps to `Either FileError Unit` with the
    // error on `Left`, so a failing unit-success operation reports `Left`
    // instead of trapping.
    let source = r#"module Main where
import Effect.Unsafe (unsafePerformEffect)
import Prelude
import Data.Either (Either(..))
import WASI.FileSystem
main :: Int
main =
  let dirs = unsafePerformEffect preopens in
  let dir = arrayIndex dirs 0 in
  case unsafePerformEffect (removeDirectoryAt (dir._1) "missing-directory") of
    Right _ -> 1
    Left _ -> 0
"#;
    let directory = std::env::temp_dir().join(format!(
        "psrs-unit-result-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let Some(output) = run_wasmtime_with_dirs(source, &[], None, &[(directory.clone(), "/data")])
    else {
        eprintln!("skipping: wasmtime is not installed");
        let _ = std::fs::remove_dir_all(&directory);
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "a unit-success failure must report Right without trapping: {output:?}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn stats_and_reads_a_directory_through_preopens_when_wasmtime_is_available() {
    // `stat`/`statAt` and a `readDirectory`/`readDirectoryEntry` walk exercise
    // the nested `option<record>` payload: `descriptor-stat` carries
    // `option<datetime>` fields, and `read-directory-entry` returns
    // `option<directory-entry>`, both stored erased inside their `Either`.
    let source = r#"module Main where
import Effect.Unsafe (unsafePerformEffect)
import Prelude
import Data.Either (Either(..))
import Data.Maybe (Maybe(..))
import WASI.Console (log)
import WASI.FileSystem
main :: Int
main =
  let dirs = unsafePerformEffect preopens in
  let dir = arrayIndex dirs 0 in
  let d = dir._1 in
  let created = unsafePerformEffect (openWrite d "probe.txt") in
  case created of
    Left _ -> 1
    Right file ->
      let wrote = unsafePerformEffect (writeString file "probe") in
      let closed = unsafePerformEffect (dropDescriptor file) in
      let stats = unsafePerformEffect (stat d) in
      case stats of
        Left _ -> 2
        Right s -> case s.type of
          Directory ->
            let fileStats = unsafePerformEffect (statAt d { symlinkFollow: true } "probe.txt") in
            case fileStats of
              Left _ -> 3
              Right f -> case f.type of
                RegularFile ->
                  let entries = unsafePerformEffect (readDirectory d) in
                  case entries of
                    Left _ -> 4
                    Right stream ->
                      let entry = unsafePerformEffect (readDirectoryEntry stream) in
                      let dropped = unsafePerformEffect (dropDirectoryEntryStream stream) in
                      case entry of
                        Left _ -> 5
                        Right Nothing -> 6
                        Right (Just _) -> let ignored = unsafePerformEffect (log "stat-ok") in 0
                _ -> 7
          _ -> 8
"#;
    let directory = std::env::temp_dir().join(format!(
        "psrs-stat-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let Some(output) = run_wasmtime_with_dirs(source, &[], None, &[(directory.clone(), "/data")])
    else {
        eprintln!("skipping: wasmtime is not installed");
        let _ = std::fs::remove_dir_all(&directory);
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"stat-ok\n", "{output:?}");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn nested_record_timestamps_roundtrip_through_adt_slots_open_rows_and_the_canonical_abi() {
    let source = r#"module Main where
import Effect.Unsafe (unsafePerformEffect)
import Prelude
import Data.Either (Either(..))
import Data.Maybe (Maybe(..))
import WASI.FileSystem

data Box a = Box a
keep :: forall a. a -> Box (Array a)
keep value = Box [value]
resize :: forall r. { size :: Int | r } -> { size :: Int | r }
resize record = record { size = 42 }
seconds :: forall r. { seconds :: Int | r } -> Int
seconds record = record.seconds
nanos :: forall r. { nanoseconds :: Int | r } -> Int
nanos record = record.nanoseconds
main :: Int
main =
  let directory = (arrayIndex (unsafePerformEffect preopens) 0)._1
      access = Timestamp { seconds: 1600000041, nanoseconds: 220000000 }
      modified = Timestamp { seconds: 1600000042, nanoseconds: 330000000 }
      changed = unsafePerformEffect (setTimesAt directory { symlinkFollow: true } "probe.bin" access modified)
  in case changed of
    Left _ -> 1
    Right _ -> case unsafePerformEffect (statAt directory { symlinkFollow: true } "probe.bin") of
      Left _ -> 2
      Right original -> case keep (resize original) of
        Box values ->
          let copied = arrayIndex values 0
          in case copied.dataAccessTimestamp, copied.dataModificationTimestamp of
            Just atime, Just mtime ->
              if intEq original.size 6 && intEq copied.size 42
                 && intEq (seconds atime) 1600000041 && intEq (nanos atime) 220000000
                 && intEq (seconds mtime) 1600000042 && intEq (nanos mtime) 330000000
              then case unsafePerformEffect (setTimesAt directory { symlinkFollow: true } "probe.bin" (Timestamp atime) (Timestamp mtime)) of
                Left _ -> 3
                Right _ -> case unsafePerformEffect (statAt directory { symlinkFollow: true } "probe.bin") of
                  Right { dataModificationTimestamp: Just returned, size: 6 } ->
                    if intEq returned.seconds 1600000042 && intEq returned.nanoseconds 330000000 then 42 else 4
                  _ -> 5
              else 6
            _, _ -> 7
"#;
    let directory = std::env::temp_dir().join(format!(
        "psrs-record-abi-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("probe.bin");
    std::fs::write(&file, b"abcdef").unwrap();
    let Some(output) = run_wasmtime_with_dirs(source, &[], None, &[(directory.clone(), "/data")])
    else {
        std::fs::remove_dir_all(directory).unwrap();
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let modified = std::fs::metadata(file)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    assert_eq!(modified.as_secs(), 1600000042);
    assert_eq!(modified.subsec_nanos(), 330000000);
    std::fs::remove_dir_all(directory).unwrap();
}

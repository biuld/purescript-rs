use super::*;

#[test]
fn writes_and_reads_a_file_through_preopens_when_wasmtime_is_available() {
    let source = r#"module Main where
import Prelude
import Data.Either (Either(..))
import WASI.Console (log)
import WASI.Filesystem
main =
  let dirs = runEffect preopens in
  let dir = arrayIndex dirs 0 in
  let created = runEffect (openWrite (dir._1) "roundtrip.txt") in
  case created of
    Left _ -> 1
    Right file ->
      let wrote = runEffect (writeString file "roundtrip") in
      let closed = runEffect (dropDescriptor file) in
      let reopened = runEffect (openRead (dir._1) "roundtrip.txt") in
      case reopened of
        Left _ -> 2
        Right reader ->
          let contents = runEffect (withDescriptor reader (\openReader -> readString openReader 64)) in
          case contents of
            Left _ -> 3
            Right text -> let ignored = runEffect (log text) in 0
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
fn reports_a_unit_success_result_as_right_when_wasmtime_is_available() {
    // DEC-13: `result<_, error-code>` maps to `Either Unit FileError`, so a
    // failing unit-success operation reports `Right` instead of trapping.
    let source = r#"module Main where
import Prelude
import Data.Either (Either(..))
import WASI.Filesystem
main :: Int
main =
  let dirs = runEffect preopens in
  let dir = arrayIndex dirs 0 in
  case runEffect (removeDirectoryAt (dir._1) "missing-directory") of
    Left _ -> 1
    Right _ -> 0
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
import Prelude
import Data.Either (Either(..))
import Data.Maybe (Maybe(..))
import WASI.Console (log)
import WASI.Filesystem
main :: Int
main =
  let dirs = runEffect preopens in
  let dir = arrayIndex dirs 0 in
  let d = dir._1 in
  let created = runEffect (openWrite d "probe.txt") in
  case created of
    Left _ -> 1
    Right file ->
      let wrote = runEffect (writeString file "probe") in
      let closed = runEffect (dropDescriptor file) in
      let stats = runEffect (stat d) in
      case stats of
        Left _ -> 2
        Right s -> case s.type of
          Directory ->
            let fileStats = runEffect (statAt d { symlinkFollow: true } "probe.txt") in
            case fileStats of
              Left _ -> 3
              Right f -> case f.type of
                RegularFile ->
                  let entries = runEffect (readDirectory d) in
                  case entries of
                    Left _ -> 4
                    Right stream ->
                      let entry = runEffect (readDirectoryEntry stream) in
                      let dropped = runEffect (dropDirectoryEntryStream stream) in
                      case entry of
                        Left _ -> 5
                        Right Nothing -> 6
                        Right (Just _) -> let ignored = runEffect (log "stat-ok") in 0
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

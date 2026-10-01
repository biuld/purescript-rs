use super::*;

#[test]
fn reads_the_wall_clock_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.Clock\n\
        main = let time = runEffect wallNow in time.seconds * 0\n";
    let artifact =
        compile_source("Main.purs", source).expect("the wall-clock wrapper should lower");
    assert!(artifact.wat.contains("wasi:clocks/wall-clock@0.2.12"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn reads_the_wall_clock_resolution_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.Clock\n\
        main = let tick = runEffect wallResolution in tick.nanoseconds * 0\n";
    let artifact = compile_source("Main.purs", source)
        .expect("the wall-clock resolution wrapper should lower");
    assert!(artifact.wat.contains("resolution"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn reads_the_insecure_seed_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.Random\n\
        main = let seed = runEffect insecureSeed in seed._1 * 0\n";
    let artifact =
        compile_source("Main.purs", source).expect("the insecure seed wrapper should lower");
    assert!(artifact.wat.contains("wasi:random/insecure-seed@0.2.12"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn reads_insecure_random_values_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.Random\n\
        main = let noise = runEffect insecureU64 in noise * 0\n";
    let artifact =
        compile_source("Main.purs", source).expect("the insecure random wrappers should lower");
    assert!(artifact.wat.contains("wasi:random/insecure@0.2.12"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn polls_a_clock_subscription_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.Clock (subscribeDuration)\n\
        import WASI.IO\n\
        main = let waiter = runEffect (subscribeDuration 0) in arrayLength (runEffect (poll [waiter])) * 0\n";
    let artifact = compile_source("Main.purs", source).expect("the poll wrapper should lower");
    assert!(artifact.wat.contains("wasi:io/poll@0.2.12"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn reads_a_pollable_readiness_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.Clock (subscribeDuration)\n\
        import WASI.IO (ready, block)\n\
        main = let waiter = runEffect (subscribeDuration 0) in let ignored = runEffect (block waiter) in if runEffect (ready waiter) then 0 else 1\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn writes_to_a_stream_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.IO\n\
        main = let out = runEffect getStdout in let ignored = runEffect (blockingWriteAndFlush out \"streamed\") in 0\n";
    let artifact =
        compile_source("Main.purs", source).expect("the stream write wrapper should lower");
    assert!(artifact.wat.contains("wasi:cli/stdout@0.2.12"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"streamed");
}

#[test]
fn gets_stdin_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.IO\n\
        main = let input = runEffect getStdin in 0\n";
    let artifact = compile_source("Main.purs", source).expect("the stdin wrapper should lower");
    assert!(artifact.wat.contains("wasi:cli/stdin@0.2.12"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn lowers_the_error_debug_string_wrapper() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.IO\n\
        main = let render = toDebugString in 0\n";
    compile_source("Main.purs", source).expect("the error debug-string wrapper should lower");
}

/// A module that declares a host `foreign import` must have an explicit export
/// list. Representation lowering suspends the host call inside the effect
/// closure, so a name on that list is the public operation. A name left off
/// the list stays private.
#[test]
fn stdlib_export_lists_do_not_expose_raw_foreign_imports() {
    let sources = crate::prelude::sources().expect("the standard library should load");
    let mut raw_imports = 0;
    let mut modules_with_raw = Vec::new();
    for module in sources {
        let imports = raw_foreign_value_names(&module.text);
        raw_imports += imports.len();
        if !imports.is_empty() {
            modules_with_raw.push(module.module_name.clone());
        }
        if imports.is_empty() {
            continue;
        }
        assert!(
            export_names(&module.text).is_some(),
            "{} has no export list and would expose its host imports",
            module.module_name
        );
    }
    assert!(
        raw_imports > 0,
        "the standard library should declare raw imports"
    );
    // Each consolidated service module owns its raw imports and exports only the
    // curated wrappers. The umbrella re-exports those modules and declares no
    // raw import itself.
    let mut expected = vec![
        "WASI.Clock",
        "WASI.FileSystem",
        "WASI.IO",
        "WASI.Network",
        "WASI.Process",
        "WASI.Random",
    ];
    expected.sort_unstable();
    modules_with_raw.sort_unstable();
    assert_eq!(
        modules_with_raw, expected,
        "only the consolidated service modules declare raw imports"
    );
    let umbrella = sources
        .iter()
        .find(|module| module.module_name == "WASI")
        .expect("the WASI umbrella module should be part of the standard library");
    assert!(
        raw_foreign_value_names(&umbrella.text).is_empty(),
        "the umbrella must declare no raw import"
    );
    assert!(
        export_names(&umbrella.text).is_some(),
        "the umbrella must have an explicit export list"
    );
}

fn export_names(text: &str) -> Option<Vec<String>> {
    let header = text.split_once("where")?.0;
    let open = header.find('(')?;
    let mut depth = 0usize;
    let mut close = None;
    for (offset, ch) in header[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let inner = &header[open + 1..close];
    Some(
        inner
            .split(',')
            .map(|entry| {
                entry
                    .trim()
                    .split(['(', ')'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
            .filter(|entry| !entry.is_empty())
            .collect(),
    )
}

fn raw_foreign_value_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("foreign import \"") else {
            continue;
        };
        let Some(close) = rest.find('"') else {
            continue;
        };
        // `psrs:effect` names the abstract effect operations (`pure`, `bind`,
        // `runEffect`). Those are the public library interface, not a private
        // host import hidden behind a wrapper.
        if rest[..close].starts_with("psrs:effect#") {
            continue;
        }
        let after = rest[close + 1..].trim_start();
        if let Some(name) = after.split_whitespace().next() {
            names.push(name.to_string());
        }
    }
    names
}

#[test]
fn lowers_a_mapped_result_with_a_variant_payload() {
    // `result<list<u8>, stream-error>` maps to `Either StreamError String`
    // (DEC-13, the error on `Left`). The `err` payload is itself a variant
    // whose case field is a resource handle, so the decode has to build the
    // nested source value before erasing it into the `Either` case field.
    let source = r#"module Main where
import Prelude
import Data.Either (Either(..))
import WASI.IO (getStdin, blockingRead, StreamError(..))
main = case runEffect (blockingRead (runEffect getStdin) 1) of
        Right bytes -> 0
        Left (LastOperationFailed _) -> 1
        Left Closed -> 2
"#;
    let artifact = compile_source("Main.purs", source)
        .expect("a result whose payload is a variant should lower");
    assert!(artifact.wat.contains("wasi:io/streams@0.2.12"));
    assert!(artifact.wat.contains("[method]input-stream.blocking-read"));
}

#[test]
fn accepts_a_newtype_resource_in_a_foreign_signature() {
    // `Resource a` is a library newtype over `Int`; the binding must erase it to
    // its handle representation through the resolved source type (DEC-14).
    let source = r#"module Main where
import Prelude
import WASI.Resource (Resource)
import WASI.IO (InputStream)
foreign import "wasi:io/streams#[method]input-stream.subscribe" subscribe :: Resource InputStream -> Int
main = 0
"#;
    compile_source("Main.purs", source)
        .expect("a newtype resource should be usable in a foreign signature");
}

#[test]
fn lowers_an_opaque_handle_resource_drop() {
    // The synthesized `[resource-drop]` intrinsic takes the opaque handle type,
    // so a source declaration may name it rather than a bare `Int`.
    let source = r#"module Main where
import Prelude
import WASI.IO (getStdin)
import WASI.IO (dropInputStream)
main = let ignored = runEffect (dropInputStream (runEffect getStdin)) in 0
"#;
    let artifact = compile_source("Main.purs", source)
        .expect("an explicit drop of the opaque input-stream handle should lower");
    assert!(artifact.wat.contains("[resource-drop]input-stream"));
}

#[test]
fn reads_stdin_with_blocking_read_when_wasmtime_is_available() {
    let source = r#"module Main where
import Prelude
import Data.Either (Either(..))
import WASI.Console (log)
import WASI.IO (getStdin)
import WASI.IO (blockingRead, StreamError(..))
main =
  let result = runEffect (blockingRead (runEffect getStdin) 5) in
  case result of
    Right text -> let ignored = runEffect (log text) in 0
    Left (LastOperationFailed _) -> 1
    Left Closed -> 2
"#;
    let artifact = compile_source("Main.purs", source).expect("blockingRead should lower");
    assert!(artifact.wat.contains("wasi:cli/stdin@0.2.12"));
    let Some(output) = run_with_wasmtime_stdin(source, b"hello world") else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"hello\n");
}

#[test]
fn lowers_the_filesystem_wrapper_surface() {
    // Every wrapper exercises a distinct WIT shape: flags and record
    // parameters, a `result<T, error-code>` result, an owned handle result,
    // and stream handles.
    let source = r#"module Main where
import Prelude
import Data.Either (Either(..))
import WASI.FileSystem
main =
  let dirs = runEffect preopens in
  let dir = arrayIndex dirs 0 in
  let d = dir._1 in
  let opened = runEffect (openRead d "a.txt") in
  let wrote = runEffect (writeFile d "x" 0) in
  let bytes = runEffect (readFile d 1 0) in
  let hash = runEffect (metadataHash d) in
  let kind = runEffect (getType d) in
  let flags = runEffect (getFlags d) in
  let target = runEffect (readlinkAt d "a") in
  let same = runEffect (isSameObject d d) in
  let stats = runEffect (stat d) in
  let statsAt = runEffect (statAt d { symlinkFollow: true } "a") in
  let entries = runEffect (readDirectory d) in
  let readEntry = case entries of
        Left _ -> 1
        Right stream ->
          let entry = runEffect (readDirectoryEntry stream) in
          let dropped = runEffect (dropDirectoryEntryStream stream) in
          0
  in
  let ignoredTimes = runEffect (setTimes d NoChange NoChange) in
  let ignoredTimesAt = runEffect (setTimesAt d { symlinkFollow: true } "a" NoChange NoChange) in
  let bracketed = runEffect (withDescriptor d (\handle -> getType handle)) in
  let ignored = runEffect (setSize d 0) in
  0
"#;
    let artifact =
        compile_source("Main.purs", source).expect("every filesystem wrapper should lower");
    assert!(artifact.wat.contains("wasi:filesystem/preopens@0.2.12"));
    assert!(artifact.wat.contains("wasi:filesystem/types@0.2.12"));
}

#[test]
fn lowers_the_sockets_wrapper_surface() {
    let source = r#"module Main where
import Prelude
import Data.Either (Either(..))
import Data.Maybe (Maybe(..))
import WASI.Network
addr :: IpSocketAddress
addr = IpV4SocketAddress { port: 0, address: { _1: 0, _2: 0, _3: 0, _4: 0 } }
main =
  let network = runEffect instanceNetwork in
  let created = runEffect (createTcpSocket Ipv4) in
  let tcp = case created of
        Right socket ->
          let ignored = runEffect (tcpStartBind socket network addr) in
          let ignoredFinish = runEffect (tcpFinishBind socket) in
          let family = runEffect (tcpAddressFamily socket) in
          let local = runEffect (tcpLocalAddress socket) in
          let remote = runEffect (tcpRemoteAddress socket) in
          let dropped = runEffect (dropTcpSocket socket) in
          0
        Left _ -> 1
  in
  let createdUdp = runEffect (createUdpSocket Ipv4) in
  let udp = case createdUdp of
        Right socket ->
          let local = runEffect (udpLocalAddress socket) in
          let remote = runEffect (udpRemoteAddress socket) in
          let streams = runEffect (udpSocketStream socket Nothing) in
          let dropped = runEffect (dropUdpSocket socket) in
          0
        Left _ -> 1
  in
  tcp + udp
"#;
    let artifact = compile_source("Main.purs", source).expect("every sockets wrapper should lower");
    assert!(
        artifact
            .wat
            .contains("wasi:sockets/instance-network@0.2.12")
    );
    assert!(
        artifact
            .wat
            .contains("wasi:sockets/tcp-create-socket@0.2.12")
    );
    assert!(artifact.wat.contains("wasi:sockets/tcp@0.2.12"));
}

#[test]
fn rejects_a_raw_filesystem_import_from_the_library() {
    let errors = check_source(
        "Main.purs",
        "module Main where\nimport WASI.FileSystem (openAtRaw)\nmain = 0\n",
    )
    .expect_err("openAtRaw is not part of the filesystem export list");
    assert!(
        errors.iter().any(|error| {
            error.message.contains("openAtRaw") && error.message.contains("not exported")
        }),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_raw_sockets_import_from_the_library() {
    let errors = check_source(
        "Main.purs",
        "module Main where\nimport WASI.Network (createTcpSocketRaw)\nmain = 0\n",
    )
    .expect_err("createTcpSocketRaw is not part of the sockets export list");
    assert!(
        errors.iter().any(|error| {
            error.message.contains("createTcpSocketRaw") && error.message.contains("not exported")
        }),
        "{errors:?}"
    );
}

#[test]
fn drops_a_stream_handle_when_wasmtime_is_available() {
    let source = r#"module Main where
import Prelude
import WASI.IO (getStdin)
import WASI.IO (dropInputStream)
main = let ignored = runEffect (dropInputStream (runEffect getStdin)) in 7
"#;
    let artifact = compile_source("Main.purs", source).expect("the drop wrapper should lower");
    assert!(artifact.wat.contains("[resource-drop]input-stream"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(7), "{output:?}");
}

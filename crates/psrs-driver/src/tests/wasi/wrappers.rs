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
        import WASI.Poll\n\
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
        import WASI.Poll (ready, block)\n\
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
        import WASI.Streams\n\
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
        import WASI.Stdin\n\
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
        import WASI.Error\n\
        main = let render = toDebugString in 0\n";
    compile_source("Main.purs", source).expect("the error debug-string wrapper should lower");
}

/// The user-facing modules export library types and wrapper functions only. A
/// raw `foreign import "<interface>#<function>"` symbol must stay module
/// private, so no export list may name it.
#[test]
fn stdlib_export_lists_do_not_expose_raw_foreign_imports() {
    let sources = crate::prelude::sources().expect("the standard library should load");
    let mut raw_imports = 0;
    for module in sources {
        let imports = raw_foreign_value_names(&module.text);
        raw_imports += imports.len();
        let exports = export_names(&module.text);
        for name in imports {
            match &exports {
                None => panic!(
                    "{} has no export list and would expose the raw import `{name}`",
                    module.module_name
                ),
                Some(exports) => assert!(
                    !exports.iter().any(|export| export == &name),
                    "{} exports the raw foreign import `{name}`",
                    module.module_name
                ),
            }
        }
    }
    assert!(
        raw_imports > 0,
        "the standard library should declare raw imports"
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
        let after = rest[close + 1..].trim_start();
        if let Some(name) = after.split_whitespace().next() {
            names.push(name.to_string());
        }
    }
    names
}

#[test]
fn lowers_a_mapped_result_with_a_variant_payload() {
    // `result<list<u8>, stream-error>` maps to `Either String StreamError`
    // (DEC-13). The `err` payload is itself a variant whose case field is an
    // opaque handle, so the decode has to build the nested source value before
    // erasing it into the `Either` case field.
    let source = r#"module Main where
import Prelude
import Data.Either (Either(..))
import WASI.Stdin (getStdin)
import WASI.Streams (blockingRead, StreamError(..))
main = case runEffect (blockingRead (runEffect getStdin) 1) of
        Left s -> 0
        Right (LastOperationFailed e) -> 1
        Right Closed -> 2
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
import WASI.Streams (InputStream)
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
import WASI.Stdin (getStdin)
import WASI.Streams (dropInputStream)
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
import WASI.Stdin (getStdin)
import WASI.Streams (blockingRead, StreamError(..))
main =
  let result = runEffect (blockingRead (runEffect getStdin) 5) in
  case result of
    Left text -> let ignored = runEffect (log text) in 0
    Right (LastOperationFailed _) -> 1
    Right Closed -> 2
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
fn drops_a_stream_handle_when_wasmtime_is_available() {
    let source = r#"module Main where
import Prelude
import WASI.Stdin (getStdin)
import WASI.Streams (dropInputStream)
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

use super::*;

mod do_notation;
mod filesystem;
mod payloads;
mod where_clause;

mod classes;
mod coercion;
mod kind_heads;
mod umbrella;
mod wrappers;

#[test]
fn lowers_string_log_to_wasi_stdout() {
    let source = "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\nimport Prelude\nimport WASI.Console\nmain = let ignored = unsafePerformEffect (log \"hello world\") in 0\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("wasi:cli/stdout@0.2.12"));
    assert!(artifact.wat.contains("wasi:io/streams@0.2.12"));
    assert!(
        artifact.wat.contains("[resource-drop]output-stream"),
        "the standard-library wrapper drops the stdout handle explicitly"
    );
    // The literal is a passive UTF-16 data segment materialized once into a
    // lazily initialized global, so the WAT holds its code units rather than
    // ASCII and guards `array.new_data` with `ref.is_null`/`global.set`.
    assert!(artifact.wat.contains("array.new_data"));
    assert!(artifact.wat.contains("ref.is_null"));
    assert!(artifact.wat.contains("global.set"));
    assert!(artifact.wat.contains("i32.load8_u"));
    assert!(artifact.wat.contains("unreachable"));
}

#[test]
fn lowers_a_source_foreign_import_with_a_wit_binding() {
    let source = "module Main where\n\
        import Prelude\n\
        foreign import \"wasi:clocks/monotonic-clock#now\" clock :: Int\n\
        main = clock * 0\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("wasi:clocks/monotonic-clock@0.2.12"));
    assert!(artifact.wat.contains("i32.wrap_i64"));
}

#[test]
fn lowers_a_boolean_wit_result_with_a_boolean_source_type() {
    let source = "module Main where\n\
        foreign import \"wasi:io/poll#[method]pollable.ready\" ready :: Int -> Boolean\n\
        main = if ready 0 then 1 else 0\n";
    let artifact = compile_source("Main.purs", source).expect("lowering a Boolean WIT result");
    assert!(artifact.wat.contains("wasi:io/poll@0.2.12"));
    assert!(artifact.wat.contains("call"));
}

#[test]
fn runs_main_as_a_wasi_component_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime("module Main where\nmain = 42\n") else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn prints_hello_world_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(
        "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\nimport Prelude\nimport WASI.Console\nmain = let ignored = unsafePerformEffect (log \"hello world\") in 0\n",
    ) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"hello world\n");
}

#[test]
fn prints_a_non_ascii_literal_when_wasmtime_is_available() {
    // `h`, `é` (U+00E9), `λ` (U+03BB): the GC UTF-16 string must be encoded as
    // UTF-8 at the canonical ABI boundary, not truncated to bytes.
    let source = "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\nimport Prelude\nimport WASI.Console\nmain = let ignored = unsafePerformEffect (log \"h\u{e9}\u{3bb}\") in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, "h\u{e9}\u{3bb}\n".as_bytes());
}

#[test]
fn prints_an_interned_literal_once_per_use_when_wasmtime_is_available() {
    let source = "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\nimport Prelude\nimport WASI.Console\nmain = let first = unsafePerformEffect (log \"twice\") in let second = unsafePerformEffect (log \"twice\") in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"twice\ntwice\n");
}

#[test]
fn prints_an_empty_literal_when_wasmtime_is_available() {
    let source = "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\nimport Prelude\nimport WASI.Console\nmain = let ignored = unsafePerformEffect (log \"\") in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"\n");
}

#[test]
fn reads_the_monotonic_clock_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(
        "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\nimport Prelude\nimport WASI.Clock\nmain = (unsafePerformEffect now) * 0\n",
    ) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn writes_to_stderr_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(
        "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\nimport Prelude\nimport WASI.Console\nmain = let x = unsafePerformEffect (error \"oops\") in 7\n",
    ) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stderr, b"oops\n");
}

#[test]
fn lowers_a_list_returning_import_with_an_allocator() {
    let source = "module Main where\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: Int -> Array Int\n\
        main = let bytes = randomBytes 8 in 0\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("wasi:random/random@0.2.12"));
    assert!(artifact.wat.contains("cabi_realloc"));
    assert!(artifact.wat.contains("i64.extend_i32_u"));
}

#[test]
fn rejects_a_string_declaration_for_a_list_of_tuples() {
    let source = "module Main where\n\
        foreign import \"wasi:cli/environment#get-environment\" env :: String\n\
        main = 0\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.stage == "P8 WIT linking"
            && error
                .message
                .contains("incompatible with its canonical result")
            && error.span.start < error.span.end
            && error.span.end <= source.len() as u32
    }));
}

#[test]
fn rejects_a_string_declaration_for_a_list_of_strings() {
    let source = "module Main where\n\
        foreign import \"wasi:cli/environment#get-arguments\" args :: String\n\
        main = 0\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.stage == "P8 WIT linking"
            && error
                .message
                .contains("incompatible with its canonical result")
    }));
}

#[test]
fn lowers_a_list_of_strings_to_an_array() {
    let source = "module Main where\n\
        foreign import \"wasi:cli/environment#get-arguments\" args :: Array String\n\
        main = arrayLength args\n";
    let artifact =
        compile_source("Main.purs", source).expect("list<string> should lower to an array");
    assert!(artifact.wat.contains("get-arguments"));
    assert!(artifact.wat.contains("array.new_default"));
}

#[test]
fn lowers_the_environment_arguments_wrapper_to_an_array() {
    let source = "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\n\
        import Prelude\n\
        import WASI.Process\n\
        main = arrayLength (unsafePerformEffect arguments)\n";
    let artifact = compile_source("Main.purs", source)
        .expect("WASI.Process.arguments should lower to an array");
    assert!(artifact.wat.contains("wasi:cli/environment@0.2.12"));
    assert!(artifact.wat.contains("get-arguments"));
    assert!(artifact.wat.contains("array.new_default"));
}

#[test]
fn rejects_an_import_of_unexported_get_arguments() {
    let errors = check_source(
        "Main.purs",
        "module Main where\nimport WASI.Process (getArguments)\nmain = 0\n",
    )
    .expect_err("getArguments is not part of the environment export list");
    assert!(
        errors.iter().any(|error| {
            error.message.contains("getArguments") && error.message.contains("not exported")
        }),
        "{errors:?}"
    );
}

#[test]
fn reads_environment_arguments_when_wasmtime_is_available() {
    // `get-arguments` returns the canonical list<string>; the wrapper recovers
    // it into a GC `Array String`. The host supplies argv, whose first element
    // is the module path under Wasmtime.
    let source = "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\n\
        import Prelude\n\
        import WASI.Process\n\
        main = arrayLength (unsafePerformEffect arguments)\n";
    let Some(output) = run_with_wasmtime_args(source, &["alpha", "beta"]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(3), "{output:?}");
}

#[test]
fn reads_environment_variables_as_records_when_wasmtime_is_available() {
    // `get-environment` returns the canonical `list<tuple<string, string>>`,
    // which maps to an `Array` of two-field records. This exercises an
    // aggregate list element end to end.
    let source = "module Main where\n\
        foreign import \"wasi:cli/environment#get-environment\" env :: Array { _1 :: String, _2 :: String }\n\
        main = arrayLength env\n";
    let artifact = compile_source("Main.purs", source)
        .expect("list<tuple<string, string>> should lower to an array of records");
    assert!(artifact.wat.contains("get-environment"));
    assert!(artifact.wat.contains("array.new_default"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn rejects_a_wit_import_when_the_declared_source_type_does_not_match() {
    let source = "module Main where\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: String -> String\n\
        main = 0\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.stage == "P8 WIT linking" && error.message.contains("incompatible type")
    }));
}

#[test]
fn rejects_a_wasi_interface_outside_the_component_capability_profile() {
    // `wasi:cli/run` is the command's export, not an import, so it is excluded
    // from the component world's import set even though it is vendored.
    let source = "module Main where\n\
        foreign import \"wasi:cli/run#run\" run :: Int\n\
        main = 0\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    let error = errors
        .iter()
        .find(|error| {
            error.stage == "P8 WIT linking"
                && error
                    .message
                    .contains("not in the current component capability profile")
        })
        .expect("an unsupported capability profile must be rejected");
    // A valid program the backend cannot support is classified as unsupported
    // source, and the diagnostic keeps the foreign import's source span.
    assert_eq!(
        error.kind,
        Some(psrs_backend::BackendErrorKind::UnsupportedSource)
    );
    assert_eq!(
        &source[error.span.start as usize..error.span.end as usize],
        "Int"
    );
}

#[test]
fn reads_random_bytes_when_wasmtime_is_available() {
    let source = "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\n\
        import Prelude\n\
        import WASI.Console\n\
        import WASI.Random\n\
        main = let noise = unsafePerformEffect randomU64 in let bytes = unsafePerformEffect (randomBytes 8) in let length = arrayLength bytes in noise * length * 0\n";
    let artifact = compile_source("Main.purs", source).expect("library randomBytes should lower");
    assert!(artifact.wat.contains("wasi:random/random@0.2.12"));
    assert!(artifact.wat.contains("get-random-bytes"));
    assert!(artifact.wat.contains("get-random-u64"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "wasmtime failed: {output:?}");
    // A WIT `list<u8>` is `Array Int`, not `String`: the bytes are returned as
    // uninterpreted integers and are never decoded as text (DEC-16).
    assert!(output.stdout.is_empty(), "{output:?}");
}

/// Native construction is inert; interpreting a reusable action executes its
/// host request and continuation on each run, preserving the returned list.
#[test]
fn stored_random_bytes_is_inert_and_repeated_runs_execute_the_continuation() {
    let program = r#"module Main where
import Prelude
import Effect.Unsafe (unsafePerformEffect)
import WASI.Console (log)
import WASI.Random (randomBytes)

action = do
  bytes <- randomBytes 8
  log "bytes"
  pure (arrayLength bytes)

"#;
    for (entry, expected_exit, expected_stdout) in [
        ("main = let stored = action in 0", 0, ""),
        (
            "main = let first = unsafePerformEffect action in let second = unsafePerformEffect action in first + second",
            16,
            "bytes\nbytes\n",
        ),
    ] {
        let source = format!("{program}{entry}\n");
        let artifact = compile_source("Main.purs", &source).expect("random action should compile");
        assert!(artifact.wat.contains("get-random-bytes"));
        let Some(output) = run_with_wasmtime(&source) else {
            eprintln!("skipping: wasmtime is not installed");
            return;
        };
        assert_eq!(output.status.code(), Some(expected_exit), "{output:?}");
        assert_eq!(output.stdout, expected_stdout.as_bytes(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
fn rejects_an_import_of_unexported_write_stdout() {
    let errors = check_source(
        "Main.purs",
        "module Main where\nimport WASI.Console (writeStdout)\nmain = 0\n",
    )
    .expect_err("writeStdout is not part of the console export list");
    assert!(
        errors.iter().any(|error| {
            error.message.contains("writeStdout") && error.message.contains("not exported")
        }),
        "{errors:?}"
    );
}

#[test]
fn passes_a_byte_array_to_another_import() {
    // A WIT `list<u8>` is `Array Int`, so a byte array crosses the boundary as
    // uninterpreted integers rather than as text (DEC-16).
    let source = "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\n\
        import Prelude\n\
        import WASI.IO (getStdout, blockingWriteAndFlush)\n\
        main = let ignored = unsafePerformEffect (bind getStdout (\\handle -> bind (blockingWriteAndFlush handle [104, 105]) (\\result -> pure 0))) in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert!(output.status.success(), "wasmtime failed: {output:?}");
    assert_eq!(output.stdout, b"hi", "{output:?}");
}

#[test]
fn keeps_multiple_returned_wit_strings_in_distinct_allocations() {
    let source = "module Main where\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: Int -> Array Int\n\
        main = let first = randomBytes 8 in let second = randomBytes 8 in 0\n";
    let artifact = compile_source("Main.purs", source).expect("lowering repeated list results");
    assert!(artifact.wat.matches("cabi_realloc").count() >= 1);
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn rejects_an_import_of_unexported_exit_with_code_raw() {
    let errors = check_source(
        "Main.purs",
        "module Main where\nimport WASI.Process (exitWithCodeRaw)\nmain = 0\n",
    )
    .expect_err("exitWithCodeRaw is not part of the exit export list");
    assert!(
        errors.iter().any(|error| {
            error.message.contains("exitWithCodeRaw") && error.message.contains("not exported")
        }),
        "{errors:?}"
    );
}

/// Storing a native exit request permits normal completion; interpreting it
/// exits with the requested code before the following log can execute.
#[test]
fn stored_exit_is_inert_and_interpreted_exit_stops_the_program() {
    let stored =
        "module Main where\nimport WASI.Process\nmain = let action = exitWithCode 99 in 0\n";
    let forced = "module Main where\nimport Effect.Unsafe (unsafePerformEffect)\nimport Prelude\nimport WASI.Process\nimport WASI.Console (log)\nmain = let value = unsafePerformEffect (exitWithCode 99) in let after = unsafePerformEffect (log \"after exit\") in 0\n";
    for (source, expected_exit) in [(stored, 0), (forced, 99)] {
        let output = run_with_wasmtime(source).expect("Wasmtime required for exit sequencing");
        assert_eq!(output.status.code(), Some(expected_exit), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
fn an_explicit_resource_drop_lowers_to_the_canonical_drop() {
    let source = "module Main where\n\
        foreign import \"wasi:cli/stdout#get-stdout\" getStdout :: Int\n\
        foreign import \"wasi:io/streams#[resource-drop]output-stream\" dropStdout :: Int -> Unit\n\
        main = let ignored = dropStdout getStdout in 0\n";
    let artifact = compile_source("Main.purs", source)
        .expect("an explicit resource drop should lower to the canonical drop");
    assert!(
        artifact.wat.contains("[resource-drop]output-stream"),
        "the drop intrinsic should be emitted"
    );
}

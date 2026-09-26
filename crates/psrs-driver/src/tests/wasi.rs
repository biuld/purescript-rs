use super::*;

#[test]
fn lowers_string_log_to_wasi_stdout() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let ignored = runEffect (log \"hello world\") in 0\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("wasi:cli/stdout@0.2.12"));
    assert!(artifact.wat.contains("wasi:io/streams@0.2.12"));
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
        "module Main where\nimport Prelude\nimport WASI.Console\nmain = let ignored = runEffect (log \"hello world\") in 0\n",
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
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let ignored = runEffect (log \"h\u{e9}\u{3bb}\") in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, "h\u{e9}\u{3bb}\n".as_bytes());
}

#[test]
fn prints_an_interned_literal_once_per_use_when_wasmtime_is_available() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let first = runEffect (log \"twice\") in let second = runEffect (log \"twice\") in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"twice\ntwice\n");
}

#[test]
fn prints_an_empty_literal_when_wasmtime_is_available() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let ignored = runEffect (log \"\") in 0\n";
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
        "module Main where\nimport Prelude\nimport WASI.Clock\nmain = (runEffect now) * 0\n",
    ) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn writes_to_stderr_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(
        "module Main where\nimport Prelude\nimport WASI.Console\nmain = let x = runEffect (error \"oops\") in 7\n",
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
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: Int -> String\n\
        main = let bytes = randomBytes 8 in 0\n";
    let artifact = compile_source("Main.purs", source).unwrap();
    assert!(artifact.wat.contains("wasi:random/random@0.2.12"));
    assert!(artifact.wat.contains("cabi_realloc"));
    assert!(artifact.wat.contains("i64.extend_i32_u"));
}

#[test]
fn rejects_a_non_byte_wit_list_before_lowering_it_as_a_string() {
    let source = "module Main where\n\
        foreign import \"wasi:cli/environment#get-arguments\" args :: String\n\
        main = 0\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.stage == "P9 MIR lowering"
            && error.message.contains("non-byte WIT list results")
            && error.span.start < error.span.end
            && error.span.end <= source.len() as u32
    }));
}

#[test]
fn rejects_a_wit_import_when_the_declared_source_type_does_not_match() {
    let source = "module Main where\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: String -> String\n\
        main = 0\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.stage == "P9 MIR lowering" && error.message.contains("incompatible type")
    }));
}

#[test]
fn rejects_a_wasi_interface_outside_the_component_capability_profile() {
    let source = "module Main where\n\
        foreign import \"wasi:random/insecure#get-insecure-random-u64\" random :: Int\n\
        main = 0\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    let error = errors
        .iter()
        .find(|error| {
            error.stage == "P9 MIR lowering"
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
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.Console\n\
        import WASI.Random\n\
        main = let noise = runEffect randomU64 in let ignored = runEffect (bind (randomBytes 8) log) in noise * 0\n";
    let artifact = compile_source("Main.purs", source).expect("library randomBytes should lower");
    assert!(artifact.wat.contains("wasi:random/random@0.2.12"));
    assert!(artifact.wat.contains("get-random-bytes"));
    assert!(artifact.wat.contains("get-random-u64"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert!(output.status.success(), "wasmtime failed: {output:?}");
    // Random bytes are decoded as UTF-8 with U+FFFD replacement. `log` appends
    // a newline to that GC string, so the payload is non-empty.
    assert!(output.stdout.ends_with(b"\n"), "{output:?}");
    assert!(output.stdout.len() > 1, "{output:?}");
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
fn passes_a_returned_wit_string_to_another_import() {
    let source = "module Main where\n\
        import Prelude\n\
        import WASI.Console\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: Int -> String\n\
        main = let ignored = runEffect (log (randomBytes 8)) in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert!(output.status.success(), "wasmtime failed: {output:?}");
    // Random bytes are decoded as UTF-8 with U+FFFD replacement, so the
    // re-encoded length is not the original byte count. The `log` call still
    // appends its newline to a non-empty payload.
    assert!(output.stdout.ends_with(b"\n"), "{output:?}");
    assert!(!output.stdout.is_empty(), "{output:?}");
}

#[test]
fn keeps_multiple_returned_wit_strings_in_distinct_allocations() {
    let source = "module Main where\n\
        foreign import \"wasi:random/random#get-random-bytes\" randomBytes :: Int -> String\n\
        main = let first = randomBytes 8 in let second = randomBytes 8 in 0\n";
    let artifact = compile_source("Main.purs", source).expect("lowering repeated list results");
    assert!(artifact.wat.matches("cabi_realloc").count() >= 1);
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
}

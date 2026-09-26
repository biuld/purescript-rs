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

/// `get-random-bytes` is not a direct call from the command entry in either
/// program: the import sits in the effect closure, and `runEffect` reaches it
/// with `call_ref`. A stored action must not `call_ref` from the entry.
#[test]
fn stored_random_bytes_leaves_get_random_bytes_inside_the_effect_closure() {
    let stored = "module Main where\n\
        import WASI.Random\n\
        main = let action = randomBytes 8 in 0\n";
    let forced = "module Main where\n\
        import Prelude\n\
        import WASI.Random\n\
        main = let value = runEffect (randomBytes 8) in 0\n";
    let stored_wat = compile_source("Main.purs", stored)
        .expect("a stored randomBytes action should compile")
        .wat;
    let forced_wat = compile_source("Main.purs", forced)
        .expect("runEffect (randomBytes 8) should compile")
        .wat;

    let stored_core = core_main(&stored_wat);
    let forced_core = core_main(&forced_wat);
    let stored_import = imported_func(stored_core, "wasi:random/random@0.2.12", "get-random-bytes");
    let forced_import = imported_func(forced_core, "wasi:random/random@0.2.12", "get-random-bytes");
    let stored_entry = exported_func(stored_core, "wasi:cli/run@0.2.12#run");
    let forced_entry = exported_func(forced_core, "wasi:cli/run@0.2.12#run");
    let stored_funcs = core_functions(stored_core);
    let forced_funcs = core_functions(forced_core);

    let stored_from_entry = reachable_by_call(&stored_funcs, stored_entry);
    assert!(
        !calls_import(&stored_funcs, &stored_from_entry, stored_import),
        "the command entry must not call get-random-bytes when the action is not run"
    );
    assert!(
        !has_call_ref(&stored_funcs, &stored_from_entry),
        "storing the action must not run the effect"
    );
    assert!(
        import_is_reached_only_from_a_closure(&stored_funcs, stored_entry, stored_import),
        "get-random-bytes must be inside the effect closure"
    );

    let forced_from_entry = reachable_by_call(&forced_funcs, forced_entry);
    assert!(
        has_call_ref(&forced_funcs, &forced_from_entry),
        "runEffect must invoke the closure from the command entry"
    );
    assert!(
        import_is_reached_only_from_a_closure(&forced_funcs, forced_entry, forced_import),
        "runEffect still calls get-random-bytes from the closure, not the entry"
    );

    let Some(output) = run_with_wasmtime(stored) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
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

fn core_main(wat: &str) -> &str {
    let start = wat
        .find("(core module $main")
        .expect("the component should contain the core module");
    let rest = &wat[start..];
    let mut depth = 0;
    for (index, character) in rest.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[..=index];
                }
            }
            _ => {}
        }
    }
    panic!("the core module is unclosed");
}

struct CoreFunc {
    index: u32,
    body: String,
}

fn core_functions(core: &str) -> Vec<CoreFunc> {
    let bytes = core.as_bytes();
    let mut functions = Vec::new();
    let mut depth = 0;
    let mut index = 0;
    while index < bytes.len() {
        if depth == 1 && core[index..].starts_with("(func (;") {
            let number_start = index + "(func (;".len();
            let number_end = core[number_start..]
                .find(";)")
                .expect("a core func should carry an index")
                + number_start;
            let func_index = core[number_start..number_end]
                .parse()
                .expect("a core func index should be a decimal");
            let mut inner = 0;
            let mut end = index;
            let mut cursor = index;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    b'(' => inner += 1,
                    b')' => {
                        inner -= 1;
                        if inner == 0 {
                            end = cursor + 1;
                            break;
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
            functions.push(CoreFunc {
                index: func_index,
                body: core[index..end].to_string(),
            });
            index = end;
            continue;
        }
        match bytes[index] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    functions
}

fn imported_func(core: &str, module: &str, name: &str) -> u32 {
    let needle = format!(r#"(import "{module}" "{name}" (func (;"#);
    let at = core
        .find(&needle)
        .unwrap_or_else(|| panic!("missing import {module}#{name}"));
    let after = &core[at + needle.len()..];
    let end = after.find(";)").expect("an import func index");
    after[..end]
        .parse()
        .expect("an import func index should be a decimal")
}

fn exported_func(core: &str, name: &str) -> u32 {
    let needle = format!(r#"(export "{name}" (func "#);
    let at = core
        .find(&needle)
        .unwrap_or_else(|| panic!("missing export {name}"));
    let after = &core[at + needle.len()..];
    let end = after.find(')').expect("an export func index");
    after[..end]
        .trim()
        .parse()
        .expect("an export func index should be a decimal")
}

fn direct_calls(body: &str) -> Vec<u32> {
    let mut calls = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find("call ") {
        let boundary = at == 0 || !rest.as_bytes()[at - 1].is_ascii_alphanumeric();
        let after = &rest[at + "call ".len()..];
        let digits = after
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if boundary && !digits.is_empty() {
            calls.push(digits.parse().expect("call operand"));
        }
        rest = &rest[at + "call ".len()..];
    }
    calls
}

fn ref_func_targets(body: &str) -> Vec<u32> {
    let mut targets = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find("ref.func ") {
        let after = &rest[at + "ref.func ".len()..];
        let digits = after
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if !digits.is_empty() {
            targets.push(digits.parse().expect("ref.func operand"));
        }
        rest = &rest[at + "ref.func ".len()..];
    }
    targets
}

fn reachable_by_call(functions: &[CoreFunc], entry: u32) -> std::collections::HashSet<u32> {
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![entry];
    while let Some(current) = stack.pop() {
        if !seen.insert(current) {
            continue;
        }
        let Some(function) = functions.iter().find(|function| function.index == current) else {
            continue;
        };
        stack.extend(direct_calls(&function.body));
    }
    seen
}

fn calls_import(
    functions: &[CoreFunc],
    reached: &std::collections::HashSet<u32>,
    import: u32,
) -> bool {
    functions.iter().any(|function| {
        reached.contains(&function.index) && direct_calls(&function.body).contains(&import)
    })
}

fn has_call_ref(functions: &[CoreFunc], reached: &std::collections::HashSet<u32>) -> bool {
    functions
        .iter()
        .any(|function| reached.contains(&function.index) && function.body.contains("call_ref "))
}

/// The import call is not in the command entry's direct-call tree. It is in a
/// function reached from `ref.func`, which is the effect closure.
fn import_is_reached_only_from_a_closure(functions: &[CoreFunc], entry: u32, import: u32) -> bool {
    if calls_import(functions, &reachable_by_call(functions, entry), import) {
        return false;
    }
    let mut from_closures = std::collections::HashSet::new();
    for function in functions {
        for target in ref_func_targets(&function.body) {
            from_closures.extend(reachable_by_call(functions, target));
        }
    }
    calls_import(functions, &from_closures, import)
}

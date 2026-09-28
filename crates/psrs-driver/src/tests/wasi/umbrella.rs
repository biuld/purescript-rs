use super::*;

#[test]
fn imports_the_wasi_umbrella_for_the_common_case() {
    // A program can `import WASI` and use the curated console, clock, and
    // random API without naming a focused module.
    let source = "module Main where\n\
        import Prelude\n\
        import WASI\n\
        main = let ignored = runEffect (log \"umbrella\") in (runEffect now) * 0\n";
    let artifact = compile_source("Main.purs", source).expect("the umbrella should compile");
    assert!(artifact.wat.contains("wasi:cli/stdout@0.2.12"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"umbrella\n");
}

#[test]
fn polls_a_resource_list_through_the_umbrella_when_wasmtime_is_available() {
    // `poll :: Array (Resource Pollable) -> Effect (Array Int)`: a resource
    // newtype as a non-byte list element must intern and lower.
    let source = "module Main where\n\
        import Prelude\n\
        import WASI\n\
        main = let waiter = runEffect (subscribeDuration 0) in arrayLength (runEffect (poll [waiter])) * 0\n";
    let artifact =
        compile_source("Main.purs", source).expect("a Resource list element should lower");
    assert!(artifact.wat.contains("wasi:io/poll@0.2.12"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

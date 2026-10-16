use super::*;

/// Official `purs` accepts this right-nested `infixr` chain. The expression
/// walks have to follow the spine on a heap stack; a native stack proportional
/// to the chain length aborts the compiler.
#[test]
fn long_boolean_conjunction_chain_returns_42() {
    let chain = std::iter::repeat_n("true", 448)
        .collect::<Vec<_>>()
        .join(" && ");
    let source = format!("module Main where\nimport Prelude\nmain = if {chain} then 42 else 1\n");
    let Some(output) = run_with_wasmtime(&source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn long_right_associative_subtraction_preserves_grouping() {
    let chain = std::iter::repeat_n("1", 448)
        .collect::<Vec<_>>()
        .join(" <-> ");
    let source = format!(
        "module Main where\nimport Prelude\ninfixr 6 difference as <->\n\
         difference x y = x - y\nmain = if ({chain}) == 0 then 42 else 1\n"
    );
    let Some(output) = run_with_wasmtime(&source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

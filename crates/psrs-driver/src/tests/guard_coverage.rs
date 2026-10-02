use super::*;

#[test]
fn shadowed_boolean_case_guard_reports_its_source_row_and_keeps_first_match() {
    let source = "module Main where\nchoose input guard = case input of\n  true -> 11\n  true | guard -> 22\n  false -> 33\nmain = choose true false\n";
    let artifact = compile_source("Main.purs", source).expect("redundancy is a warning");
    let redundant = artifact
        .warnings
        .iter()
        .filter(|warning| {
            warning
                .diagnostic
                .message
                .contains("redundant case alternative")
        })
        .collect::<Vec<_>>();

    assert_eq!(redundant.len(), 1, "{:?}", artifact.warnings);
    assert_eq!(redundant[0].source, 0);
    assert_eq!(
        &source[redundant[0].diagnostic.span.start as usize
            ..redundant[0].diagnostic.span.end as usize],
        "true | guard -> 22"
    );

    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(11), "{output:?}");
}

#[test]
fn guarded_boolean_rows_do_not_shadow_each_other_or_change_fallthrough() {
    let source = "module Main where\ninvert value = case value of\n  true -> false\n  false -> true\nchoose first = case true of\n  true | first -> 11\n  true | invert first -> 22\n  true -> 23\n  false -> 33\nmain = choose false\n";
    let artifact = compile_source("Main.purs", source).expect("guarded Boolean rows compile");
    assert!(
        artifact.warnings.iter().all(|warning| {
            !warning
                .diagnostic
                .message
                .contains("redundant case alternative")
                && !warning.diagnostic.message.contains("non-exhaustive case")
        }),
        "guarded rows do not cover or shadow one another: {:?}",
        artifact.warnings
    );

    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(22), "{output:?}");
}

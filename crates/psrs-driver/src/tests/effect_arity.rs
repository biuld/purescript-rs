//! Calling-convention evidence for the effect representation.
//!
//! `Effect τ` is a one-parameter closure. A source arrow such as
//! `String -> Effect Unit` stays saturated at that arrow; the token is not
//! another parameter of `log`.

use super::run_with_wasmtime;

#[test]
fn an_effect_of_a_function_is_not_arity_two_and_log_is_saturated() {
    let source = "module Main where\nimport Prelude\nimport WASI.Clock\nimport WASI.Console\nmk :: Effect (Int -> Int)\nmk = pure (\\x -> x + 1)\nmain = let stored = now in let message = log \"message\" in runEffect (bind mk (\\f -> pure (f 41)))\n";
    let core = crate::lower_source_to_core("Main.purs", source).expect("source lowers to Core");
    let stages = psrs_backend::compile_with_stages(core).expect("Core lowers through CC");
    let function = |name: &str| {
        stages
            .cc
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("missing function {name}"))
    };
    assert_eq!(
        function("log").parameters.len(),
        1,
        "log keeps the source arrow and leaves the token on the returned closure"
    );
    assert_eq!(
        function("now").parameters.len(),
        1,
        "Effect Int is a one-parameter closure"
    );
    assert_eq!(
        function("pure").parameters.len(),
        1,
        "pure takes the value and returns the token closure"
    );
    assert_eq!(
        function("bind").parameters.len(),
        2,
        "bind takes the effect and the continuation; the token belongs to the result"
    );
    assert_ne!(
        function("mk").parameters.len(),
        2,
        "an alias of Effect (Int -> Int) is not a two-parameter function"
    );
    let log_symbol = function("log").symbol;
    let log_calls = stages
        .cc
        .functions
        .iter()
        .flat_map(|function| direct_calls(&function.assignments))
        .filter(|(symbol, _)| *symbol == log_symbol)
        .map(|(_, arguments)| arguments)
        .collect::<Vec<_>>();
    assert!(
        !log_calls.is_empty() && log_calls.iter().all(|count| *count == 1),
        "log \"message\" is a saturated one-argument call, got {log_calls:?}"
    );
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "storing log \"message\" must not run it: {output:?}"
    );
}

/// `pick :: Boolean -> String -> Effect Unit` has two source parameters. Supplying
/// only the Boolean captures that argument once and returns `String -> Effect Unit`.
/// The token stays on the effect that `pick` returns, and the write waits until
/// that effect is run.
#[test]
fn a_partial_source_application_captures_once_and_defers_the_effect() {
    let stored = "module Main where\nimport Prelude\nimport WASI.Console\npick :: Boolean -> String -> Effect Unit\npick choice message = if choice then log message else log \"other\"\nmain = let partial = pick true in 0\n";
    let stages = psrs_backend::compile_with_stages(
        crate::lower_source_to_core("Main.purs", stored).expect("source lowers to Core"),
    )
    .expect("Core lowers through CC");
    let pick = stages
        .cc
        .functions
        .iter()
        .find(|function| function.name == "pick")
        .expect("pick");
    assert_eq!(
        pick.parameters.len(),
        2,
        "the token is not a parameter of pick"
    );
    let partials = stages
        .cc
        .functions
        .iter()
        .filter(|function| {
            function.name.starts_with("partial_")
                && direct_calls(&function.assignments)
                    .iter()
                    .any(|(symbol, arguments)| *symbol == pick.symbol && *arguments == 2)
        })
        .collect::<Vec<_>>();
    assert_eq!(partials.len(), 1, "one partial application of pick");
    assert_eq!(
        partials[0].parameters.len(),
        2,
        "the partial keeps the remaining String and the closure environment"
    );
    let captures = partials[0]
        .assignments
        .iter()
        .filter(|assignment| {
            matches!(
                assignment.kind,
                psrs_backend::cc::AssignmentKind::ClosureGetCapture { index: 0, .. }
            )
        })
        .count();
    assert_eq!(captures, 1, "the supplied Boolean is captured once");
    let Some(stored_output) = run_with_wasmtime(stored) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(stored_output.status.code(), Some(0), "{stored_output:?}");
    assert!(
        stored_output.stdout.is_empty(),
        "building the partial application must not run the effect: {stored_output:?}"
    );

    let ran = "module Main where\nimport Prelude\nimport WASI.Console\npick :: Boolean -> String -> Effect Unit\npick choice message = if choice then log message else log \"other\"\nmain = let partial = pick true in let first = runEffect (partial \"again\") in let second = runEffect (partial \"again\") in 0\n";
    let Some(ran_output) = run_with_wasmtime(ran) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(ran_output.status.code(), Some(0), "{ran_output:?}");
    assert_eq!(ran_output.stdout, b"again\nagain\n", "{ran_output:?}");
}

fn direct_calls(assignments: &[psrs_backend::cc::Assignment]) -> Vec<(psrs_hir::SymbolId, usize)> {
    let mut found = Vec::new();
    for assignment in assignments {
        match &assignment.kind {
            psrs_backend::cc::AssignmentKind::DirectCall {
                function,
                arguments,
            } => found.push((*function, arguments.len())),
            psrs_backend::cc::AssignmentKind::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                found.extend(direct_calls(then_assignments));
                found.extend(direct_calls(else_assignments));
            }
            psrs_backend::cc::AssignmentKind::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    found.extend(direct_calls(&case.assignments));
                }
                found.extend(direct_calls(default_assignments));
            }
            _ => {}
        }
    }
    found
}

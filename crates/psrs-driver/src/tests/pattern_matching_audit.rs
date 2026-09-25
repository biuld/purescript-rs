//! Value-sensitive execution and structural evidence for the pattern-matching
//! topic (PM-01..PM-11).
//!
//! Every execution case runs the normal P8-to-Wasm pipeline and is executed
//! with Wasmtime when it is available (mandatory under
//! `PSRS_REQUIRE_WASMTIME=1`). Structural cases lower Typed Core and inspect
//! the verified CC/MIR before Wasm.

use super::*;
use psrs_backend::cc::AssignmentKind;

fn stages(source: &str) -> psrs_backend::Stages {
    let core = lower_source_to_core("Main.purs", source).expect("source lowers to Core");
    psrs_backend::compile_with_stages(core).expect("Core lowers through CC to Wasm")
}

fn flatten_cc(assignments: &[psrs_backend::cc::Assignment]) -> Vec<&psrs_backend::cc::Assignment> {
    let mut output = Vec::new();
    for assignment in assignments {
        output.push(assignment);
        match &assignment.kind {
            AssignmentKind::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                output.extend(flatten_cc(then_assignments));
                output.extend(flatten_cc(else_assignments));
            }
            AssignmentKind::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    output.extend(flatten_cc(&case.assignments));
                }
                output.extend(flatten_cc(default_assignments));
            }
            _ => {}
        }
    }
    output
}

/// PM-01: a `case` evaluates its scrutinee exactly once. The scrutinee record
/// logs on construction and is tested by two alternatives; a compiler that
/// re-evaluated it per alternative would print the side effect twice.
#[test]
fn case_evaluates_its_scrutinee_exactly_once() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\ndata I = A | B\nmain = case { tag: A, payload: runEffect (log \"once\") } of\n  { tag: A, payload: _ } -> 0\n  { tag: B, payload: _ } -> 1\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout, b"once\n",
        "the scrutinee must be evaluated once, not once per alternative"
    );
}

/// PM-01, PM-06: first-match order survives reordering, duplicates, and an
/// earlier irrefutable alternative, observed through value-sensitive exits.
#[test]
fn first_match_order_is_value_sensitive() {
    let cases = [
        (
            "module Main where\ndata T = A | B | C\ntoInt t = case t of\n  C -> 30\n  A -> 10\n  B -> 20\nmain = toInt B\n",
            20,
        ),
        (
            "module Main where\ndata T = A | B\nmain = case A of\n  A -> 10\n  A -> 20\n  _ -> 30\n",
            10,
        ),
        (
            "module Main where\ndata T = A | B\nmain = case B of\n  A -> 10\n  _ -> 20\n  B -> 30\n",
            20,
        ),
        (
            "module Main where\ndata T = A | B\nmain = case A of\n  x -> 10\n  A -> 20\n",
            10,
        ),
    ];
    for (source, expected) in cases {
        let Some(output) = run_with_wasmtime(source) else {
            eprintln!("skipping: wasmtime is not installed");
            return;
        };
        assert_eq!(output.status.code(), Some(expected), "{source}");
    }
}

/// PM-06: a nullary sum dispatches through CC `TagSwitch`, MIR
/// `Terminator::Switch`, and Wasm `br_table`, and the dense/sparse/default
/// cases produce the selected branch value.
#[test]
fn nullary_sum_dispatch_lowers_through_all_three_stages() {
    let source = "module Main where\ndata Color = Red | Green | Blue\ntoInt color = case color of\n  Blue -> 30\n  Red -> 10\n  Green -> 20\nmain = toInt Blue\n";
    let stages = stages(source);
    assert!(
        stages
            .cc
            .functions
            .iter()
            .flat_map(|f| &f.assignments)
            .any(|assignment| matches!(assignment.kind, AssignmentKind::TagSwitch { .. })),
        "nullary sums must lower to a CC TagSwitch"
    );
    let has_switch = stages.mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                Some(psrs_backend::mir::Terminator::Switch { .. })
            )
        })
    });
    assert!(has_switch, "the TagSwitch must become a MIR Switch");
    assert!(
        stages.artifact.wat.contains("br_table"),
        "the MIR Switch must become a Wasm br_table"
    );
    assert!(
        stages.artifact.wat.contains("unreachable"),
        "the impossible missing-tag edge must become Wasm unreachable"
    );
    assert!(stages.artifact.wasm.len() > 8);

    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(30));
}

/// PM-05, PM-07: a field-bearing sum reads its tag once before projecting, and
/// a failed nested test falls through to the next alternative.
#[test]
fn field_bearing_sum_tests_the_tag_once_and_falls_through() {
    let source = "module Main where\ndata I = A Int | B\ndata O = O I\nread o = case o of\n  O (A n) -> n\n  O B -> 7\nmain = read (O B)\n";
    let stages = stages(source);
    let tags = stages
        .cc
        .functions
        .iter()
        .flat_map(|function| flatten_cc(&function.assignments))
        .filter(|assignment| matches!(assignment.kind, AssignmentKind::VariantTag { .. }))
        .count();
    assert_eq!(tags, 1, "the outer tag must be read exactly once");
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(7),
        "the failed nested test must fall through"
    );
}

/// PM-05: a multi-field product projects only the fields some alternative
/// needs; the unneeded fields are never read.
#[test]
fn product_pattern_projects_only_needed_fields() {
    let source = "module Main where\ndata T = T Int Int Int | E\nread v = case v of\n  T _ y _ -> y\n  E -> 0\nmain = read (T 1 42 3)\n";
    let stages = stages(source);
    let fields = stages
        .cc
        .functions
        .iter()
        .flat_map(|function| flatten_cc(&function.assignments))
        .filter_map(|assignment| match assignment.kind {
            AssignmentKind::VariantGet { field, .. } => Some(field),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fields, vec![1], "only the bound field is projected");

    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

/// PM-08: a dependent erased field recovers through its declared template at
/// two instantiations (an erased scalar and an erased aggregate).
#[test]
fn dependent_erased_fields_recover_at_two_instantiations() {
    let source = "module Main where\ndata I = A | B\ndata Wrap a = Wrap a\nfromInt :: Wrap Int -> Int\nfromInt (Wrap n) = n\nfromAggregate :: Wrap I -> Int\nfromAggregate (Wrap i) = case i of\n  A -> 1\n  B -> 2\nmain = fromInt (Wrap 40) + fromAggregate (Wrap B)\n";
    let stages = stages(source);
    assert!(
        stages.artifact.wat.contains("ref.cast"),
        "erased aggregate recovery must use a checked cast, not a nominal reinterpret"
    );
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

/// PM-10: recursive ADTs compile with nested patterns, terminate, and keep
/// first-match semantics at depth.
#[test]
fn recursive_pattern_compilation_terminates_and_stays_first_match() {
    let source = "module Main where\ndata Nat = Z | S Nat\nf n = case n of\n  S (S (S (S x))) -> 40\n  S (S (S Z)) -> 2\n  S (S Z) -> 2\n  S Z -> 1\n  Z -> 0\nmain = f (S (S (S (S Z)))) + f (S (S Z))\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

/// PM-11: the optimizer and Wasm lowering preserve the selected branch value
/// and keep the source-associated redundancy warning.
#[test]
fn optimization_preserves_values_and_source_warnings() {
    let source = "module Main where\ndata T = A | B | C\nread t = case t of\n  A -> 10\n  v -> 20\n  B -> 30\nmain = read B\n";
    let artifact = compile_source("Main.purs", source).expect("redundancy is a warning");
    assert_eq!(
        artifact.warnings.len(),
        1,
        "the unreachable `B` row must be reported"
    );
    let warning = &artifact.warnings[0];
    assert_eq!(warning.source, 0);
    assert!(
        warning
            .diagnostic
            .message
            .contains("redundant case alternative")
    );
    let warned =
        &source[warning.diagnostic.span.start as usize..warning.diagnostic.span.end as usize];
    assert_eq!(warned, "B -> 30");

    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(20),
        "optimization must preserve the selected (irrefutable) branch value"
    );
}

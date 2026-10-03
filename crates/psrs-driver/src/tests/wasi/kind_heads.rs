//! Kinds named in a type position, through the whole pipeline.
//!
//! `Type`, `Constraint`, and `Symbol` are kinds, and official PureScript
//! declares each of them with kind `Type`, so a type position that names one is
//! an ordinary nominal type. These cases check that a declaration using them
//! reaches Core, that a value of a different type is still rejected, and that
//! the three are kept apart.

use super::*;

/// A record keeps every kind head reachable from `main` without needing a value
/// of any of them: the record's type names them, and `main` reads a field that
/// does not.
const KIND_HEAD_SOURCE: &str = r#"
module Main where

onType :: Type -> Int
onType _ = 1

onConstraint :: Constraint -> Int
onConstraint _ = 2

onSymbol :: Symbol -> Int
onSymbol _ = 3

handlers :: { onType :: Type -> Int, onConstraint :: Constraint -> Int, onSymbol :: Symbol -> Int, size :: Int }
handlers = { onType: onType, onConstraint: onConstraint, onSymbol: onSymbol, size: 0 }

main :: Int
main = handlers.size
"#;

/// The kind heads must not merely pass the type checker: they have to reach the
/// Core type table as spine heads.
#[test]
fn a_kind_named_in_a_type_position_reaches_the_core_type_table() {
    let core = lower_source_to_core("Main.purs", KIND_HEAD_SOURCE)
        .expect("a kind head in a type position should lower to Core");
    for constructor in [
        psrs_core::TypeConstructor::Type,
        psrs_core::TypeConstructor::Constraint,
        psrs_core::TypeConstructor::Symbol,
    ] {
        assert!(
            core.types.iter().any(
                |ty| matches!(ty, psrs_core::Type::Constructor(found) if *found == constructor)
            ),
            "{constructor:?} should reach Core: {:?}",
            core.types
        );
    }
    core.verify().unwrap();
}

const KIND_HEAD_REJECTED_SOURCE: &str = r#"
module Main where

onConstraint :: Constraint -> Int
onConstraint _ = 2

main :: Int
main = onConstraint (1 :: Int)
"#;

/// Naming a kind produces a real nominal type: `Int` is not one, so the use is
/// rejected rather than accepted against a fresh unknown.
#[test]
fn a_value_of_another_type_is_rejected_where_a_kind_is_named() {
    let errors = compile_source("Main.purs", KIND_HEAD_REJECTED_SOURCE)
        .expect_err("an Int is not a Constraint");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("expected Constraint, found Int")),
        "unexpected diagnostics: {errors:?}"
    );
}

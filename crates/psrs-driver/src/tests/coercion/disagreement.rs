//! Corpus coercions whose determined failure is `TypesDoNotUnify`.
//!
//! Official `decompose` reports that code for an unequal nominal argument, and
//! `canonRow` reports it for closed rows whose labels do not align. These are
//! the six L4 mismatches that previously fell through to `NoInstanceFound`.

fn rejects(source: &str) {
    let errors = crate::check_program(&[("Main.purs", source)]).expect_err("coercion should fail");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TypesDoNotUnify")),
        "expected TypesDoNotUnify, got {errors:?}"
    );
}

#[test]
fn a_nominal_parameter_does_not_unify() {
    rejects(
        r#"module Main where
import Safe.Coerce (coerce)
data Nominal a (b :: Type) = Nominal a
type role Nominal nominal phantom
nominalToNominal :: forall a b c. Nominal a c -> Nominal b c
nominalToNominal = coerce
"#,
    );
}

#[test]
fn a_nominal_argument_under_a_type_application_does_not_unify() {
    rejects(
        r#"module Main where
import Safe.Coerce (coerce)
data Phantom a = Phantom
data Maybe a = Nothing | Just a
data G a b = G (a (Phantom b))
gToG :: G Maybe Int -> G Maybe String
gToG = coerce
"#,
    );
}

#[test]
fn a_nominal_wrapped_argument_does_not_unify() {
    rejects(
        r#"module Main where
import Safe.Coerce (coerce)
data Nominal a (b :: Type) = Nominal a
type role Nominal nominal phantom
newtype Id a = Id a
data Wrap a b = Wrap (Nominal a b)
wrapToWrap :: forall a b. Wrap a b -> Wrap (Id a) b
wrapToWrap = coerce
"#,
    );
}

#[test]
fn closed_rows_with_different_labels_do_not_unify() {
    rejects(
        r#"module Main where
import Safe.Coerce (coerce)
recToRec :: { x :: Int } -> { y :: String }
recToRec = coerce
"#,
    );
}

#[test]
fn a_nominal_constrained_parameter_does_not_unify() {
    rejects(
        r#"module Main where
import Safe.Coerce (coerce)
class Unary a
data Constrained a = Constrained (Unary a => a)
constrainedToConstrained :: forall a b. Constrained a -> Constrained b
constrainedToConstrained = coerce
"#,
    );
}

#[test]
fn a_nominal_constrained_parameter_does_not_unify_with_a_newtype() {
    rejects(
        r#"module Main where
import Safe.Coerce (coerce)
class Unary a
data Constrained a = Constrained (Unary a => a)
newtype N a = N a
constrainedToConstrained :: forall a. Constrained a -> Constrained (N a)
constrainedToConstrained = coerce
"#,
    );
}

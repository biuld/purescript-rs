//! Visible type application `e @T`.
//!
//! Official `TypeChecker/Types.hs` substitutes the written argument for the
//! operand's outermost *visible* quantifier after checking the argument against
//! that quantifier's kind, and erases the application. `e @_` consumes a
//! quantifier without choosing a type for it.
//!
//! `purs` 0.15.16's CST/Convert.hs makes a binder visible only when the source
//! wrote `forall @a.`; a plain `forall a.` binder becomes `TypeVarInvisible` and
//! is instantiated before a visible application can select it, so `purs`
//! answers `f @Int` for `f :: forall a. a -> a` with
//! `CannotApplyExpressionOfTypeOnType`. This compiler's CST does not yet carry
//! that visibility — see `docs/design/frontend/type-system/type-inference.md` —
//! so a visible application here selects a plain `forall a.` binder too. That
//! is a known divergence in the permissive direction and is pinned by
//! `a_visible_application_accepts_a_plain_forall_binder_that_purs_rejects`.
//! Everything else below was checked against `purs` 0.15.16.

/// The `errorCode` the driver's type-check stage reports, so a case states which
/// obligation failed.
fn rejection(source_name: &str, source: &str) -> Vec<(u32, &'static str)> {
    let errors =
        psrs_driver::check_source(source_name, source).expect_err("the case must be rejected");
    errors
        .iter()
        .map(|error| (error.span.start, error.code.unwrap_or("<none>")))
        .collect()
}

fn accepts(source_name: &str, source: &str) {
    if let Err(errors) = psrs_driver::check_source(source_name, source) {
        panic!("{source_name} was rejected: {errors:#?}");
    }
}

/// A visible application supplies the argument inference would otherwise leave
/// open, and it is erased: the value is the operand.
#[test]
fn applies_a_visible_argument_to_a_global_and_erases() {
    accepts(
        "vta-global.purs",
        r#"module Main where
import Prelude

identity2 :: forall @a. a -> a
identity2 x = x

both :: forall @a @b. a -> b -> Int
both _ _ = 7

main :: Int
main =
  let
    atInt = identity2 @Int 1
    atOther = identity2 @Int 2
    isSeven = both @Int 0 0
  in
    if atInt == 1 then (if atOther == 2 then (if isSeven == 7 then 1 else 0) else 0) else 0
"#,
    );
}

/// A constructor's own scheme is a scheme like any other, so a visible
/// application selects its parameters the same way.
#[test]
fn applies_a_visible_argument_to_a_constructor() {
    accepts(
        "vta-constructor.purs",
        r#"module Main where

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

main :: Int
main = case Proxy @Int of _ -> 0
"#,
    );
}

/// `@_` consumes a quantifier without choosing a type for it. A later
/// application stepping past it to the *next* quantifier is a chain, which this
/// compiler does not yet resolve; see
/// `a_chained_visible_application_is_reported_rather_than_resolved`.
#[test]
fn a_wildcard_argument_consumes_a_quantifier() {
    accepts(
        "vta-skip.purs",
        r#"module Main where

both :: forall @a @b. a -> b -> Int
both _ _ = 7

main :: Int
main = both @_ 1 2
"#,
    );
}

/// A partial application instantiates only the quantifier it names and leaves
/// the rest, so the result is still a function of the other parameter.
#[test]
fn a_partial_application_leaves_the_other_quantifiers_alone() {
    accepts(
        "vta-partial.purs",
        r#"module Main where

both :: forall @a @b. a -> b -> Int
both _ _ = 7

main :: Int
main = both @Int 0 0
"#,
    );
}

/// A chain selects a quantifier an earlier application left behind. Choosing
/// between the remaining ones needs the scheme to record which variables a
/// visible application has consumed, which it does not yet do, so the chain is
/// reported instead of resolved against a binder list no other stage would share.
#[test]
fn a_chained_visible_application_is_reported_rather_than_resolved() {
    let errors = psrs_driver::check_source(
        "vta-chain.purs",
        r#"module Main where

both :: forall @a @b. a -> b -> Int
both _ _ = 7

main :: Int
main = both @Int @Int 0 0
"#,
    )
    .expect_err("a chain must be reported rather than resolved");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("chained visible type application")),
        "the diagnostic must name the chain it cannot resolve: {errors:#?}"
    );
}

/// Official reports these two separately, and this compiler uses the same codes:
/// an argument with nothing to apply to, and a skip with nothing to skip.
#[test]
fn an_operand_without_a_quantifier_is_rejected_per_official() {
    let applied = rejection(
        "vta-no-quantifier.purs",
        r#"module Main where

main :: Int
main = 1 @Int
"#,
    );
    assert!(
        applied
            .iter()
            .any(|(_, code)| *code == "CannotApplyExpressionOfTypeOnType"),
        "applying to a monomorphic operand must be official's error: {applied:?}"
    );

    let skipped = rejection(
        "vta-no-quantifier-skip.purs",
        r#"module Main where

main :: Int
main = 1 @_
"#,
    );
    assert!(
        skipped
            .iter()
            .any(|(_, code)| *code == "CannotSkipTypeApplication"),
        "skipping a monomorphic operand must be official's error: {skipped:?}"
    );
}

/// A second application on the same line is a chain, and is reported as one
/// rather than resolved: `purs` accepts `identity2 @Int @Int` only because it
/// keeps the quantifier left behind rigid, which this compiler does not model.
#[test]
fn a_second_application_on_one_line_is_reported_as_a_chain() {
    let errors = psrs_driver::check_source(
        "vta-twice.purs",
        r#"module Main where

identity2 :: forall @a. a -> a
identity2 x = x

main :: Int
main = identity2 @Int @Int 1
"#,
    )
    .expect_err("a chain must be reported");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("chained visible type application")),
        "the diagnostic must name the chain it cannot resolve: {errors:#?}"
    );
}

/// Known divergence, pinned so it cannot change unnoticed. `purs` 0.15.16 makes
/// a plain `forall a.` binder invisible and answers this with
/// `CannotApplyExpressionOfTypeOnType`; this compiler's CST does not carry that
/// visibility yet, so it accepts the program. The acceptance is in the permissive
/// direction — the program is still typechecked — and it is the reason a visible
/// application must not be read as agreement with `purs` until the binder's
/// visibility is modelled.
#[test]
fn a_visible_application_accepts_a_plain_forall_binder_that_purs_rejects() {
    accepts(
        "vta-plain-binder.purs",
        r#"module Main where

identity2 :: forall a. a -> a
identity2 x = x

main :: Int
main = case identity2 @Int 1 of _ -> 0
"#,
    );
}

/// The application selects a type argument; it does not change the value, so the
/// program still produces the operand's own result at runtime.
#[test]
fn a_visible_application_still_produces_a_component_artifact() {
    let source = r#"module Main where
import Prelude

identity2 :: forall @a. a -> a
identity2 x = x

both :: forall @a @b. a -> b -> Int
both _ _ = 7

main :: Int
main =
  let
    atInt = identity2 @Int 41
    skipped = both @_ 0 1
  in
    if atInt == 41 then (if skipped == 7 then 1 else 0) else 0
"#;
    let Ok(artifact) = psrs_driver::compile_source("vta-erases", source) else {
        panic!("the program must compile");
    };
    assert!(
        !artifact.wasm.is_empty(),
        "a visible application must still produce a component artifact"
    );
}

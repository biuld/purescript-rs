//! Visible type application `e @T`.
//!
//! Official `TypeChecker/Types.hs` substitutes the written argument for the
//! operand's outermost *visible* quantifier after checking the argument against
//! that quantifier's kind, and erases the application. `e @_` consumes a
//! quantifier without choosing a type for it.
//!
//! A binder is visible only when the source wrote `forall @a.`. A plain
//! `forall a.` binder is instantiated before a visible application can select
//! it. `@T` selects the next visible binder and leaves the ones it did not
//! name quantified, so `f @A @B` and `f @_ @B` both resolve. These cases were
//! checked against `purs` 0.15.16.

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

/// `@_` makes the outermost visible binder invisible without substituting it.
/// The following value arguments instantiate that binder.
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

/// Class parameters are the visible prefix, so `constClass @Int @Number`
/// selects both and a data parameter is selected the same way.
#[test]
fn a_class_head_chain_selects_each_parameter() {
    accepts(
        "vta-class-head.purs",
        r#"module Main where

class ConstClass a where
  constClass :: forall @b. a -> b -> a

instance ConstClass a where
  constClass a _ = a

constClassInt = constClass @Int @Number

data Tree a = Leaf a | Branch (Tree a) (Tree a)

treeInt :: Int -> Tree Int
treeInt = Leaf @Int

main :: Int
main = constClassInt 1 0.0
"#,
    );
}

/// A chain selects the quantifier an earlier application left quantified.
#[test]
fn a_chained_visible_application_selects_the_next_binder() {
    accepts(
        "vta-chain.purs",
        r#"module Main where

both :: forall @a @b. a -> b -> Int
both _ _ = 7

main :: Int
main = both @Int @Int 0 0
"#,
    );
}

/// `@_` steps over one visible binder so the next `@T` selects the one after it.
#[test]
fn a_skip_then_a_visible_argument_selects_the_next_binder() {
    accepts(
        "vta-skip-chain.purs",
        r#"module Main where

both :: forall @a @b. a -> b -> Int
both _ _ = 7

main :: Int
main = both @_ @Int 0 0
"#,
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

/// A second application with no quantifier left is
/// `CannotApplyExpressionOfTypeOnType`. `purs` does not keep a consumed
/// binder around for another `@T`.
#[test]
fn a_second_application_with_no_binder_left_is_rejected() {
    let errors = rejection(
        "vta-twice.purs",
        r#"module Main where

identity2 :: forall @a. a -> a
identity2 x = x

main :: Int
main = identity2 @Int @Int 1
"#,
    );
    assert!(
        errors
            .iter()
            .any(|(_, code)| *code == "CannotApplyExpressionOfTypeOnType"),
        "a consumed binder cannot take another argument: {errors:?}"
    );
}

/// A plain `forall a.` binder is invisible, so `@Int` has nothing to select.
#[test]
fn a_plain_forall_binder_is_not_selectable() {
    let errors = rejection(
        "vta-plain-binder.purs",
        r#"module Main where

identity2 :: forall a. a -> a
identity2 x = x

main :: Int
main = case identity2 @Int 1 of _ -> 0
"#,
    );
    assert!(
        errors
            .iter()
            .any(|(_, code)| *code == "CannotApplyExpressionOfTypeOnType"),
        "a plain forall binder must be official's error: {errors:?}"
    );
}

/// The three corpus programs named by issue #123, checked through the shipped
/// entry with the annotated `errorCode`.
#[test]
fn corpus_visible_type_application_failures_use_the_annotated_codes() {
    let invisible = rejection(
        "VisibleTypeApplications1.purs",
        include_str!("../../../tests/upstream/failing/VisibleTypeApplications1.purs"),
    );
    assert!(
        invisible
            .iter()
            .any(|(_, code)| *code == "CannotApplyExpressionOfTypeOnType"),
        "VisibleTypeApplications1: {invisible:?}"
    );
    let monomorphic = rejection(
        "VisibleTypeApplications2.purs",
        include_str!("../../../tests/upstream/failing/VisibleTypeApplications2.purs"),
    );
    assert!(
        monomorphic
            .iter()
            .any(|(_, code)| *code == "CannotApplyExpressionOfTypeOnType"),
        "VisibleTypeApplications2: {monomorphic:?}"
    );
    let class_head = rejection(
        "ClassHeadNoVTA3.purs",
        include_str!("../../../tests/upstream/failing/ClassHeadNoVTA3.purs"),
    );
    assert!(
        class_head
            .iter()
            .any(|(_, code)| *code == "NoInstanceFound"),
        "ClassHeadNoVTA3: {class_head:?}"
    );
    assert!(
        class_head
            .iter()
            .all(|(_, code)| *code == "NoInstanceFound"),
        "ClassHeadNoVTA3 must not report a second code: {class_head:?}"
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

/// A visible argument is checked against its quantifier's kind, not against
/// kind `Type`. `Symbol` and `Type -> Type` are both legal arguments.
#[test]
fn a_visible_argument_may_have_a_non_type_kind() {
    accepts(
        "vta-symbol.purs",
        r#"module Main where

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

symbol :: forall (@s :: Symbol). Proxy s -> Int
symbol _ = 0

higher :: forall (@f :: Type -> Type). Proxy f -> Int
higher _ = 1

useSymbol :: Int
useSymbol = symbol @"hi" (Proxy @"hi")

useHigher :: Int
useHigher = higher @Array (Proxy @Array)

main :: Int
main = useSymbol
"#,
    );
}

/// An argument whose kind is not the quantifier's kind is a kind mismatch.
#[test]
fn a_visible_argument_of_the_wrong_kind_is_rejected() {
    let errors = rejection(
        "vta-kind-mismatch.purs",
        r#"module Main where

identity :: forall (@a :: Type). a -> a
identity x = x

main :: Int
main = identity @"hi" 0
"#,
    );
    assert!(
        errors.iter().any(|(_, code)| *code == "KindsDoNotUnify"),
        "a Symbol argument does not have kind Type: {errors:?}"
    );
}

/// Dropping the `Type` equation must not drop the undefined-variable check.
#[test]
fn a_visible_argument_still_rejects_an_undefined_variable() {
    let errors = rejection(
        "vta-undefined.purs",
        r#"module Main where

identity :: forall @a. a -> a
identity x = x

main :: Int
main = identity @b 0
"#,
    );
    assert!(
        errors
            .iter()
            .any(|(_, code)| *code == "UndefinedTypeVariable"),
        "an undefined argument must still be named: {errors:?}"
    );
}

/// One instantiation shares a kind variable across the constructor's
/// parameters. A later argument of a different kind is a mismatch.
#[test]
fn one_constructor_use_shares_its_kind_variable() {
    let errors = rejection(
        "vta-shared-kind.purs",
        r#"module Main where

data Pair :: forall k. k -> k -> Type
data Pair a b = Pair

main :: Int
main = case Pair @Int @"no" of _ -> 0
"#,
    );
    assert!(
        errors.iter().any(|(_, code)| *code == "KindsDoNotUnify"),
        "Int and a Symbol do not share k: {errors:?}"
    );
}

/// A quantifier kind `forall a. a -> a` opens before it is checked, so a
/// matching constructor and a partially applied one are legal arguments.
#[test]
fn a_visible_argument_may_have_a_quantified_kind() {
    accepts(
        "vta-quantified-kind.purs",
        r#"module Main where

foreign import data Id :: forall (a :: Type). a -> a

identityCheck :: forall (@f :: forall (a :: Type). a -> a). Int
identityCheck = 0

foreign import data Const :: forall a b. a -> b -> a

constCheck :: forall (a :: Type) (@f :: forall (b :: Type). b -> a). Int
constCheck = 0

identityPass :: Int
identityPass = identityCheck @Id

constPass :: Int
constPass = constCheck @(Const Int)

main :: Int
main = identityPass
"#,
    );
}

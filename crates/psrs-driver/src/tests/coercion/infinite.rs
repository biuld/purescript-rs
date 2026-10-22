//! Coercible solver cycles.
//!
//! A newtype that unwraps to a type which decomposes back into the same wanted
//! is `PossiblyInfiniteCoercibleInstance`. A newtype that wraps itself directly
//! is not: unwrapping is declined and ordinary decomposition runs, so a given
//! `Coercible a b` still discharges `N a -> N b`.

#[test]
fn a_newtype_that_unwraps_back_to_the_same_wanted_is_infinite() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype N a = N (a -> N a)\n\
        infinite :: forall a b. N a -> N b\n\
        infinite = coerce\n";
    let errors = crate::check_program(&[("Main.purs", source)]).expect_err("coercion should fail");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("PossiblyInfiniteCoercibleInstance")),
        "expected PossiblyInfiniteCoercibleInstance, got {errors:?}"
    );
    assert!(
        errors
            .iter()
            .all(|error| error.diagnostic.code != Some("NoInstanceFound")),
        "the infinite code replaces NoInstanceFound, got {errors:?}"
    );
}

#[test]
fn a_self_wrapped_newtype_uses_decomposition_instead_of_the_infinite_code() {
    let source = "module Main where\n\
        import Safe.Coerce (class Coercible, coerce)\n\
        newtype N a = N (N a)\n\
        type role N representational\n\
        good :: forall a b. Coercible a b => N a -> N b\n\
        good = coerce\n";
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("a given should discharge the decomposed newtype: {errors:?}")
    });
}

#[test]
fn a_self_wrapped_newtype_without_a_given_is_not_infinite() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype N a = N (N a)\n\
        type role N representational\n\
        bad :: forall a b. N a -> N b\n\
        bad = coerce\n";
    let errors = crate::check_program(&[("Main.purs", source)]).expect_err("coercion should fail");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
        "expected NoInstanceFound, got {errors:?}"
    );
    assert!(
        errors
            .iter()
            .all(|error| error.diagnostic.code != Some("PossiblyInfiniteCoercibleInstance")),
        "direct self-wrap is not the infinite code, got {errors:?}"
    );
}

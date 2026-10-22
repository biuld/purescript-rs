//! Kind checking of a `Coercible` constraint when the solver picks it up.
//!
//! Official `Entailment/Coercible.hs` elaborates both argument kinds and
//! unifies them, including a pair role decomposition produces. The written
//! signature stays kind `Type`; the mismatch is not a declaration-kind error.

#[test]
fn a_decomposed_polykind_coercion_reports_kinds_do_not_unify() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        data D :: forall k. k -> Type\n\
        data D a = D\n\
        type role D representational\n\
        example :: forall (a :: Type) (b :: Symbol). D a -> D b\n\
        example = coerce\n\
        main :: Int\n\
        main = 0\n";
    let errors = crate::check_program(&[("Main.purs", source)])
        .expect_err("a representational polykind mismatch is a kind error");
    assert!(
        errors.iter().any(|error| {
            error.diagnostic.code == Some("KindsDoNotUnify")
                && error.diagnostic.stage.contains("typecheck")
        }),
        "the solver should report the decomposed pair: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .all(|error| !error.diagnostic.stage.contains("kind check")),
        "the signature is well-kinded: {errors:?}"
    );
}

#[test]
fn an_expression_ascription_coercion_reports_kinds_do_not_unify() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        data D :: forall k. k -> Type\n\
        data D a = D\n\
        type role D representational\n\
        example = (coerce :: D Int -> D \"hi\")\n\
        main :: Int\n\
        main = 0\n";
    let errors = crate::check_program(&[("Main.purs", source)])
        .expect_err("an ascribed polykind mismatch is a kind error");
    assert!(
        errors.iter().any(|error| {
            error.diagnostic.code == Some("KindsDoNotUnify")
                && error.diagnostic.stage.contains("typecheck")
        }),
        "the ascription reaches the solver: {errors:?}"
    );
}

#[test]
fn a_phantom_polykind_coercion_is_accepted() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        data D :: forall k. k -> Type\n\
        data D a = D\n\
        example :: D Int -> D \"hi\"\n\
        example = coerce\n\
        main :: Int\n\
        main = 0\n";
    crate::check_program(&[("Main.purs", source)])
        .unwrap_or_else(|errors| panic!("a phantom parameter may change kind: {errors:?}"));
}

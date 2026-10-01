fn rejects(sources: &[(&str, &str)], expected_code: Option<&str>) {
    let errors = crate::check_program(sources).expect_err("program should be rejected");
    if let Some(expected_code) = expected_code {
        assert!(
            errors.iter().any(|error| {
                error.diagnostic.code == Some(expected_code)
                    || (expected_code == "NoInstanceFound"
                        && error
                            .diagnostic
                            .message
                            .contains("no instance for constraint"))
            }),
            "expected {expected_code}, got {errors:?}"
        );
    }
}

#[test]
fn infers_roles_and_accepts_phantom_coercion() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        data Phantom a = Phantom\n\
        convert :: Phantom Int -> Phantom Boolean\n\
        convert = coerce\n\
        main :: Int\n\
        main = let ignored = convert Phantom in 42\n";
    crate::check_program(&[("Main.purs", source)])
        .unwrap_or_else(|errors| panic!("phantom roles should permit coercion: {errors:?}"));
}

#[test]
fn expands_type_synonyms_before_coercible_role_matching() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Age = Age Int\n\
        type AgeAlias = Age\n\
        asInt :: AgeAlias -> Int\n\
        asInt = coerce\n\
        main :: Int\n\
        main = asInt (Age 79)\n";
    crate::check_program(&[("Main.purs", source)])
        .unwrap_or_else(|errors| panic!("synonym coercion should type check: {errors:?}"));
}

#[test]
fn a_nominal_data_role_blocks_newtype_coercion_of_its_parameter() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        data Box a = Box a\n\
        type role Box nominal\n\
        newtype Age = Age Int\n\
        bad :: Box Age -> Box Int\n\
        bad = coerce\n\
        main :: Int\n\
        main = 0\n";
    rejects(&[("Main.purs", source)], Some("NoInstanceFound"));
}

#[test]
fn an_inferred_representational_data_role_allows_visible_newtype_coercion() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        data Box a = Box a\n\
        newtype Age = Age Int\n\
        unbox (Box value) = value\n\
        main :: Int\n\
        main = unbox (coerce (Box (Age 109)))\n";
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("inferred representational role should coerce: {errors:?}")
    });
}

#[test]
fn a_nominal_newtype_role_still_allows_visible_double_unwrapping() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Box a = Box a\n\
        type role Box nominal\n\
        newtype Age = Age Int\n\
        unbox (Box value) = value\n\
        main :: Int\n\
        main = unbox (coerce (Box (Age 127)))\n";
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("visible newtype unwrapping should precede roles: {errors:?}")
    });
}

#[test]
fn composes_given_coercible_constraints_transitively() {
    let source = "module Main where\n\
        import Safe.Coerce (class Coercible, coerce)\n\
        newtype Age = Age Int\n\
        newtype Raw = Raw Int\n\
        convert :: forall a b c. Coercible a b => Coercible b c => a -> b -> c\n\
        convert value _ = coerce value\n\
        main :: Int\n\
        main = convert (Age 131) (Raw 0)\n";
    crate::check_program(&[("Main.purs", source)])
        .unwrap_or_else(|errors| panic!("given coercions should compose transitively: {errors:?}"));
}

#[test]
fn rewrites_canonical_given_constraints_through_representational_roles() {
    let source = "module Main where\n\
        import Safe.Coerce (class Coercible, coerce)\n\
        data D a b = D a\n\
        rewrite :: forall a b d e. Coercible a (D b e) => Coercible b d => a -> D d e\n\
        rewrite = coerce\n\
        main :: Int\n\
        main = 42\n";
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("canonical givens should rewrite representational arguments: {errors:?}")
    });
}

#[test]
fn rewrites_a_canonical_given_in_a_higher_kinded_application_head() {
    let source = "module Main where\n\
        import Safe.Coerce (class Coercible, coerce)\n\
        rewrite :: forall f g a b. Coercible a (f b) => Coercible f g => a -> g b\n\
        rewrite = coerce\n\
        main :: Int\n\
        main = 42\n";
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("higher-kinded canonical givens should rewrite the application head: {errors:?}")
    });
}

#[test]
fn aligns_canonical_open_record_rows_before_proving_field_coercions() {
    let source = "module Main where\n\
        import Safe.Coerce (class Coercible, coerce)\n\
        newtype Age = Age Int\n\
        convert :: forall r s. Coercible r s => { left :: Age, value :: Age | r } -> { value :: Int, left :: Int | s }\n\
        convert = coerce\n\
        main :: Int\n\
        main = 42\n";
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("canonical open-row tails and fields should be aligned: {errors:?}")
    });
}

#[test]
fn rejects_open_record_rows_with_different_known_labels() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        bad :: forall r s. { x :: Int | r } -> { y :: Int | s }\n\
        bad = coerce\n\
        main :: Int\n\
        main = 0\n";
    rejects(&[("Main.purs", source)], Some("NoInstanceFound"));
}

#[test]
fn does_not_rewrite_a_noncanonical_recursive_given() {
    let source = "module Main where\n\
        import Safe.Coerce (class Coercible, coerce)\n\
        data D a = D a\n\
        bad :: forall a b. Coercible b (D b) => a -> b\n\
        bad = coerce\n\
        main :: Int\n\
        main = 42\n";
    rejects(&[("Main.purs", source)], Some("NoInstanceFound"));
}

#[test]
fn composes_explicit_noncanonical_given_proofs_transitively() {
    let source = r#"module Main where
import Prim.Coerce (class Coercible)
import Safe.Coerce (coerce)
data D a = D a
bad :: forall a b. Coercible a b => Coercible b (D b) => a -> D b
bad = coerce
main :: Int
main = 0
"#;
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("explicit given coercion proofs should compose: {errors:?}")
    });
}

#[test]
fn interacts_canonical_givens_with_a_shared_left_variable() {
    let source = r#"module Main where
import Prim.Coerce (class Coercible)
import Safe.Coerce (coerce)

data D a = D a

rewrite :: forall a b. Coercible a (D b) => Coercible a (D Int) => D b -> D Int
rewrite = coerce

main :: Int
main = 42
"#;
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("same-variable canonical givens should interact: {errors:?}")
    });
}

#[test]
fn expands_an_imported_synonym_inside_a_data_field_before_role_inference() {
    let library = "module Lib (Alias, Box(..)) where\n\
        type Alias a = a\n\
        data Box a = Box (Alias a)\n";
    let main = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        import Lib (Alias, Box(..))\n\
        newtype Age = Age Int\n\
        unbox (Box value) = value\n\
        main :: Int\n\
        main = unbox (coerce (Box (Age 137)))\n";
    crate::check_program(&[("Lib.purs", library), ("Main.purs", main)]).unwrap_or_else(|errors| {
        panic!("synonym payload should infer a representational role: {errors:?}")
    });
}

#[test]
fn rejects_a_role_annotation_that_weakens_inference() {
    let source = "module Main where\n\
        data Box a = Box a\n\
        type role Box phantom\n\
        main :: Int\n\
        main = 0\n";
    rejects(&[("Main.purs", source)], Some("RoleMismatch"));
}

#[test]
fn reports_role_arity_mismatch_on_a_data_declaration() {
    let source = "module Main where\n\
        data Box a = Box a\n\
        type role Box nominal phantom\n\
        main :: Int\n\
        main = 0\n";
    rejects(
        &[("Main.purs", source)],
        Some("RoleDeclarationArityMismatch"),
    );
}

#[test]
fn reports_an_orphan_role_declaration_during_surface_lowering() {
    let source = "module Main where\n\
        type role Box representational\n\
        data Box a = Box a\n\
        main :: Int\n\
        main = 0\n";
    rejects(&[("Main.purs", source)], Some("OrphanRoleDeclaration"));
}

#[test]
fn rejects_user_defined_coercible_instances() {
    let source = "module Main where\n\
        import Prim.Coerce (class Coercible)\n\
        instance unsafeCoerce :: Coercible Int Boolean\n\
        main :: Int\n\
        main = 0\n";
    rejects(
        &[("Main.purs", source)],
        Some("InvalidCoercibleInstanceDeclaration"),
    );
}

#[test]
fn compiler_coercion_intrinsic_is_only_in_scope_through_safe_coerce() {
    let source = "module Main where\n\
        main :: Int\n\
        main = __psrs_coerce 42\n";
    rejects(&[("Main.purs", source)], Some("UnknownName"));
}

#[test]
fn an_imported_newtype_requires_its_constructor_for_unwrapping() {
    let library = "module Lib (Age, age) where\n\
        newtype Age = Age Int\n\
        age :: Int -> Age\n\
        age n = Age n\n";
    let consumer = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        import Lib (Age, age)\n\
        main :: Int\n\
        main = coerce (age 42)\n";
    rejects(
        &[("Lib.purs", library), ("Main.purs", consumer)],
        Some("NoInstanceFound"),
    );
}

#[test]
fn imported_role_metadata_restricts_coercion() {
    let library = "module Lib (Box(..)) where\n\
        data Box a = Box a\n\
        type role Box nominal\n";
    let consumer = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        import Lib (Box(..))\n\
        newtype Age = Age Int\n\
        bad :: Box Age -> Box Int\n\
        bad = coerce\n\
        main :: Int\n\
        main = 0\n";
    rejects(
        &[("Lib.purs", library), ("Main.purs", consumer)],
        Some("NoInstanceFound"),
    );
}

#[test]
fn foreign_data_roles_default_to_nominal_and_accept_explicit_signatures() {
    let conservative = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        foreign import data Opaque :: Type -> Type\n\
        bad :: Opaque Int -> Opaque Boolean\n\
        bad = coerce\n\
        main :: Int\n\
        main = 0\n";
    rejects(&[("Main.purs", conservative)], Some("NoInstanceFound"));

    let annotated = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        import Prim.Coerce (class Coercible)\n\
        foreign import data Opaque :: Type -> Type\n\
        type role Opaque representational\n\
        change :: forall a b. Coercible a b => Opaque a -> Opaque b\n\
        change = coerce\n\
        main :: Int\n\
        main = 0\n";
    crate::check_program(&[("Main.purs", annotated)])
        .unwrap_or_else(|errors| panic!("explicit foreign roles are trusted: {errors:?}"));
}

#[test]
fn rejects_coercion_between_type_constructors_with_different_kinds() {
    let source = r#"module Main where

import Safe.Coerce (coerce)

data Unary a
data Binary a b

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

type role Proxy representational

bad :: Proxy Unary -> Proxy Binary
bad = coerce
"#;
    rejects(&[("Main.purs", source)], Some("NoInstanceFound"));
}

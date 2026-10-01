use super::*;

#[test]
fn differential_role_and_coercible_rules_against_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    let coercion_module = "module Safe.Coerce where\n\
        import Prim.Coerce (class Coercible)\n\
        foreign import unsafeCoerce :: forall a b. a -> b\n\
        coerce :: forall a b. Coercible a b => a -> b\n\
        coerce = unsafeCoerce\n";

    let local_nominal = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        data Box a = Box a\n\
        type role Box nominal\n\
        newtype Age = Age Int\n\
        bad :: Box Age -> Box Int\n\
        bad = coerce\n";
    let imported_nominal_lib = "module Lib (Box(..)) where\n\
        data Box a = Box a\n\
        type role Box nominal\n";
    let imported_nominal_main = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        import Lib (Box(..))\n\
        newtype Age = Age Int\n\
        bad :: Box Age -> Box Int\n\
        bad = coerce\n";
    let representational_data = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        data Box a = Box a\n\
        newtype Age = Age Int\n\
        unbox (Box value) = value\n\
        main :: Int\n\
        main = unbox (coerce (Box (Age 109)))\n";
    let phantom_sum = r#"module Main where
import Safe.Coerce (coerce)
data Phantom a = Empty | Marked Int
convert :: Phantom Int -> Phantom Boolean
convert = coerce
main :: Int
main = case convert (Marked 42) of
  Empty -> 0
  Marked value -> value
"#;
    let nominal_newtype = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Box a = Box a\n\
        type role Box nominal\n\
        newtype Age = Age Int\n\
        unbox (Box value) = value\n\
        main :: Int\n\
        main = unbox (coerce (Box (Age 127)))\n";
    let hidden_lib = "module HiddenLib (Age, age) where\n\
        newtype Age = Age Int\n\
        age :: Int -> Age\n\
        age = Age\n";
    let hidden_main = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        import HiddenLib (Age, age)\n\
        main :: Int\n\
        main = coerce (age 42)\n";
    let weakened_role = "module Main where\n\
        data Box a = Box a\n\
        type role Box phantom\n\
        main :: Int\n\
        main = 0\n";
    let alias_lib = "module BoxLib (Alias, Box(..)) where\n\
        type Alias a = a\n\
        data Box a = Box (Alias a)\n";
    let alias_main = "module Main where\n\
        import BoxLib (Alias, Box(..))\n\
        import Safe.Coerce (coerce)\n\
        newtype Age = Age Int\n\
        unbox (Box value) = value\n\
        main :: Int\n\
        main = unbox (coerce (Box (Age 137)))\n";
    let transitive_official = "module Main where\n\
        import Prim.Coerce (class Coercible)\n\
        import Safe.Coerce (coerce)\n\
        newtype Age = Age Int\n\
        newtype Raw = Raw Int\n\
        convert :: forall a b c. Coercible a b => Coercible b c => a -> b -> c\n\
        convert value _ = coerce value\n\
        main :: Int\n\
        main = convert (Age 131) (Raw 0)\n";
    let transitive_compiler = "module Main where\n\
        import Safe.Coerce (class Coercible, coerce)\n\
        newtype Age = Age Int\n\
        newtype Raw = Raw Int\n\
        convert :: forall a b c. Coercible a b => Coercible b c => a -> b -> c\n\
        convert value _ = coerce value\n\
        main :: Int\n\
        main = convert (Age 131) (Raw 0)\n";
    let parameterized_scalar = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Box a = Box a\n\
        main :: Int\n\
        main = coerce (Box 89)\n";
    let parameterized_function = r#"module Main where
import Safe.Coerce (coerce)
newtype Endo a = Endo (a -> a)
coerceEndo :: Endo Int -> Int -> Int
coerceEndo = coerce
main :: Int
main = coerceEndo (Endo (\value -> value)) 37
"#;
    let parameterized_array = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Items a = Items (Array a)\n\
        unwrap :: Items Int -> Array Int\n\
        unwrap = coerce\n\
        main :: Int\n\
        main = 0\n";

    let canonical_role_rewrite = "module Main where\n\
        import Prim.Coerce (class Coercible)\n\
        import Safe.Coerce (coerce)\n\
        data D a b = D a\n\
        rewrite :: forall a b d e. Coercible a (D b e) => Coercible b d => a -> D d e\n\
        rewrite = coerce\n";
    let canonical_higher_kinded_rewrite = "module Main where\n\
        import Prim.Coerce (class Coercible)\n\
        import Safe.Coerce (coerce)\n\
        rewrite :: forall f g a b. Coercible a (f b) => Coercible f g => a -> g b\n\
        rewrite = coerce\n";
    let canonical_open_row = "module Main where\n\
        import Prim.Coerce (class Coercible)\n\
        import Safe.Coerce (coerce)\n\
        newtype Age = Age Int\n\
        convert :: forall r s. Coercible r s => { left :: Age, value :: Age | r } -> { value :: Int, left :: Int | s }\n\
        convert = coerce\n";
    let incompatible_open_row = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        bad :: forall r s. { left :: Int | r } -> { right :: Int | s }\n\
        bad = coerce\n";
    let noncanonical_recursive_given = "module Main where\n\
        import Prim.Coerce (class Coercible)\n\
        import Safe.Coerce (coerce)\n\
        data D a = D a\n\
        bad :: forall a b. Coercible b (D b) => a -> b\n\
        bad = coerce\n";
    let noncanonical_transitive_given = "module Main where\n\
        import Prim.Coerce (class Coercible)\n\
        import Safe.Coerce (coerce)\n\
        data D a = D a\n\
        bad :: forall a b. Coercible a b => Coercible b (D b) => a -> D b\n\
        bad = coerce\n";

    let kind_mismatch = r#"module Main where
import Safe.Coerce (coerce)
data Unary a
data Binary a b
data Proxy :: forall k. k -> Type
data Proxy a = Proxy
type role Proxy representational
bad :: Proxy Unary -> Proxy Binary
bad = coerce
"#;
    let same_variable_interaction = r#"module Main where
import Prim.Coerce (class Coercible)
import Safe.Coerce (coerce)
data D a = D a
rewrite :: forall a b. Coercible a (D b) => Coercible a (D Int) => D b -> D Int
rewrite = coerce
"#;
    let cases: [CoercionDifferentialCase<'_>; 20] = [
        (
            "role-local-nominal",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", local_nominal),
            ],
            &[("Main.purs", local_nominal)],
            false,
        ),
        (
            "role-imported-nominal",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Lib.purs", imported_nominal_lib),
                ("Main.purs", imported_nominal_main),
            ],
            &[
                ("Lib.purs", imported_nominal_lib),
                ("Main.purs", imported_nominal_main),
            ],
            false,
        ),
        (
            "coercible-representational-data",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", representational_data),
            ],
            &[("Main.purs", representational_data)],
            true,
        ),
        (
            "coercible-phantom-sum",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", phantom_sum),
            ],
            &[("Main.purs", phantom_sum)],
            true,
        ),
        (
            "coercible-visible-nominal-newtype",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", nominal_newtype),
            ],
            &[("Main.purs", nominal_newtype)],
            true,
        ),
        (
            "coercible-hidden-newtype",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("HiddenLib.purs", hidden_lib),
                ("Main.purs", hidden_main),
            ],
            &[("HiddenLib.purs", hidden_lib), ("Main.purs", hidden_main)],
            false,
        ),
        (
            "role-weakening",
            &[("Main.purs", weakened_role)],
            &[("Main.purs", weakened_role)],
            false,
        ),
        (
            "coercible-imported-alias-data",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("BoxLib.purs", alias_lib),
                ("Main.purs", alias_main),
            ],
            &[("BoxLib.purs", alias_lib), ("Main.purs", alias_main)],
            true,
        ),
        (
            "coercible-transitive-givens",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", transitive_official),
            ],
            &[("Main.purs", transitive_compiler)],
            true,
        ),
        (
            "coercible-parameterized-newtype-scalar",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", parameterized_scalar),
            ],
            &[("Main.purs", parameterized_scalar)],
            true,
        ),
        (
            "coercible-parameterized-newtype-function",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", parameterized_function),
            ],
            &[("Main.purs", parameterized_function)],
            true,
        ),
        (
            "coercible-parameterized-newtype-array",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", parameterized_array),
            ],
            &[("Main.purs", parameterized_array)],
            true,
        ),
        (
            "coercible-canonical-role-rewrite",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", canonical_role_rewrite),
            ],
            &[("Main.purs", canonical_role_rewrite)],
            true,
        ),
        (
            "coercible-canonical-higher-kinded-rewrite",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", canonical_higher_kinded_rewrite),
            ],
            &[("Main.purs", canonical_higher_kinded_rewrite)],
            true,
        ),
        (
            "coercible-canonical-open-row-alignment",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", canonical_open_row),
            ],
            &[("Main.purs", canonical_open_row)],
            true,
        ),
        (
            "coercible-incompatible-open-row-labels",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", incompatible_open_row),
            ],
            &[("Main.purs", incompatible_open_row)],
            false,
        ),
        (
            "coercible-noncanonical-recursive-given",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", noncanonical_recursive_given),
            ],
            &[("Main.purs", noncanonical_recursive_given)],
            false,
        ),
        (
            "coercible-explicit-noncanonical-givens-compose",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", noncanonical_transitive_given),
            ],
            &[("Main.purs", noncanonical_transitive_given)],
            true,
        ),
        (
            "coercible-kind-mismatch",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", kind_mismatch),
            ],
            &[("Main.purs", kind_mismatch)],
            false,
        ),
        (
            "coercible-same-variable-given-interaction",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", same_variable_interaction),
            ],
            &[("Main.purs", same_variable_interaction)],
            true,
        ),
    ];

    let mut failures = Vec::new();
    for (name, official_sources, compiler_sources, expected_acceptance) in cases {
        let purs_accepted = purs_accepts_sources(name, official_sources);
        let psrs_result = psrs_driver::check_program(compiler_sources);
        let psrs_accepted = psrs_result.is_ok();
        if purs_accepted != expected_acceptance {
            failures.push(format!(
                "`{name}`: purs accepted={purs_accepted}, expected={expected_acceptance}"
            ));
        }
        if psrs_accepted != expected_acceptance {
            failures.push(format!(
                "`{name}`: psrs accepted={psrs_accepted}, expected={expected_acceptance}; errors={:?}",
                psrs_result.err()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "role/Coercible differential failures:\n{}",
        failures.join("\n")
    );
}

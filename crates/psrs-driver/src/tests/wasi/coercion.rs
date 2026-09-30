use super::super::*;

#[test]
fn unwraps_nested_visible_newtypes_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Raw = Raw Int\n\
        newtype UserId = UserId Raw\n\
        main :: Int\n\
        main = coerce (UserId (Raw 47))\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(47));
}

#[test]
fn converts_a_parameterized_newtype_to_its_scalar_payload_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Box a = Box a\n\
        main :: Int\n\
        main = coerce (Box 89)\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(89));
}

#[test]
fn converts_a_parameterized_newtype_to_its_function_payload_when_wasmtime_is_available() {
    let source = r#"module Main where
import Safe.Coerce (coerce)
newtype Endo a = Endo (a -> a)
coerceEndo :: Endo Int -> Int -> Int
coerceEndo = coerce
main :: Int
main = coerceEndo (Endo (\value -> value + 5)) 37
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn converts_a_parameterized_newtype_to_its_array_payload_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Items a = Items (Array a)\n\
        unwrap :: Items Int -> Array Int\n\
        unwrap = coerce\n\
        main :: Int\n\
        main = arrayIndex (unwrap (Items [43])) 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(43));
}

#[test]
fn adapts_a_function_between_newtype_and_representation_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Age = Age Int\n\
        ageValue :: Age -> Int\n\
        ageValue (Age value) = value\n\
        main :: Int\n\
        main = (coerce ageValue) 59\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(59));
}

#[test]
fn maps_an_array_element_coercion_when_wasmtime_is_available() {
    let source = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Age = Age Int\n\
        main :: Int\n\
        main = arrayIndex (coerce [Age 67]) 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(67));
}

#[test]
fn maps_an_inferred_representational_data_field_when_wasmtime_is_available() {
    let source = r#"module Main where
import Safe.Coerce (coerce)
data Box a = Box a
newtype Age = Age Int
unbox boxed = case boxed of
  Box value -> value
main :: Int
main = unbox (coerce (Box (Age 109)))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(109));
}

#[test]
fn preserves_a_phantom_sum_constructor_tag_when_wasmtime_is_available() {
    let source = r#"module Main where
import Safe.Coerce (coerce)
data Phantom a = Empty | Marked Int
convert :: Phantom Int -> Phantom Boolean
convert = coerce
main :: Int
main = case convert (Marked 42) of
  Empty -> 0
  Marked value -> value
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn unwraps_both_sides_before_applying_a_nominal_newtype_role_when_wasmtime_is_available() {
    let source = r#"module Main where
import Safe.Coerce (coerce)
newtype Box a = Box a
type role Box nominal
newtype Age = Age Int
unbox boxed = case boxed of
  Box value -> value
main :: Int
main = unbox (coerce (Box (Age 127)))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(127));
}

#[test]
fn uses_a_generic_coercion_function_from_an_imported_module_when_wasmtime_is_available() {
    let library = "module CoerceLib where\n\
        import Safe.Coerce (class Coercible, coerce)\n\
        convert :: forall a b. Coercible a b => a -> b\n\
        convert = coerce\n\
        convertArray :: forall a b. Coercible a b => Array a -> Array b\n\
        convertArray = coerce\n";
    let main = "module Main where\n\
        import CoerceLib (convert, convertArray)\n\
        import Safe.Coerce (class Coercible)\n\
        newtype Age = Age Int\n\
        age :: Age\n\
        age = Age 71\n\
        main :: Int\n\
        main = convert age + arrayIndex (convertArray [age]) 0\n";
    let Some(output) =
        run_program_with_wasmtime(&[("CoerceLib.purs", library), ("Main.purs", main)])
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(142));
}

#[test]
fn coerces_through_an_imported_visible_newtype_when_wasmtime_is_available() {
    let library = "module AgeLib (Age(..), age) where\n\
        newtype Age = Age Int\n\
        age :: Int -> Age\n\
        age value = Age value\n";
    let main = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        import AgeLib (Age(..), age)\n\
        main :: Int\n\
        main = coerce (age 83)\n";
    let Some(output) = run_program_with_wasmtime(&[("AgeLib.purs", library), ("Main.purs", main)])
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(83));
}

#[test]
fn uses_transitive_inferred_roles_from_imported_modules_when_wasmtime_is_available() {
    let base = "module Base (Box(..)) where\n\
        data Box a = Box a\n";
    let layer = "module Layered (Layer(..)) where\n\
        import Base (Box(..))\n\
        data Layer a = Layer (Box a)\n";
    let main = "module Main where\n\
        import Base (Box(..))\n\
        import Safe.Coerce (coerce)\n\
        import Layered (Layer(..))\n\
        newtype Age = Age Int\n\
        extract :: Layer Int -> Int\n\
        extract (Layer (Box value)) = value\n\
        main :: Int\n\
        main = extract (coerce (Layer (Box (Age 97))))\n";
    let Some(output) = run_program_with_wasmtime(&[
        ("Base.purs", base),
        ("Layered.purs", layer),
        ("Main.purs", main),
    ]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(97));
}

#[test]
fn composes_imported_coercible_givens_in_a_generic_function_when_wasmtime_is_available() {
    let library = "module CoerceLib where\n\
        import Safe.Coerce (class Coercible, coerce)\n\
        convert :: forall a b c. Coercible a b => Coercible b c => a -> b -> c\n\
        convert value _ = coerce value\n";
    let main = "module Main where\n\
        import CoerceLib (convert)\n\
        import Safe.Coerce (class Coercible)\n\
        newtype Age = Age Int\n\
        newtype Raw = Raw Int\n\
        main :: Int\n\
        main = convert (Age 131) (Raw 0)\n";
    let Some(output) =
        run_program_with_wasmtime(&[("CoerceLib.purs", library), ("Main.purs", main)])
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(131));
}

#[test]
fn expands_an_imported_synonym_in_a_data_field_when_wasmtime_is_available() {
    let library = r#"module BoxLib (Alias, Box(..)) where
type Alias a = a
data Box a = Box (Alias a)
"#;
    let main = r#"module Main where
import BoxLib (Alias, Box(..))
import Safe.Coerce (coerce)
newtype Age = Age Int
unbox boxed = case boxed of
  Box value -> value
main :: Int
main = unbox (coerce (Box (Age 137)))
"#;
    let Some(output) = run_program_with_wasmtime(&[("BoxLib.purs", library), ("Main.purs", main)])
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(137));
}

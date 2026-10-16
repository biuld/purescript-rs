//! A constructorless algebraic data type is uninhabited: no expression builds
//! one, so its only values come from unchecked coercions, and the backend lays
//! it out as an erased reference. This exercises a type (like the reference
//! `VariantF`) that has no constructors of its own yet carries a coerced
//! concrete representation.

#[test]
fn a_constructorless_data_type_round_trips_through_unsafe_coercion() {
    let source = r#"module Main where

import Prelude
import Unsafe.Coerce (unsafeCoerce)

data Empty :: Type -> Type
data Empty a

newtype Rep a = Rep { value :: a }

toEmpty :: forall a. Rep a -> Empty a
toEmpty = unsafeCoerce

fromEmpty :: forall a. Empty a -> Rep a
fromEmpty = unsafeCoerce

main :: Int
main = case fromEmpty (toEmpty (Rep { value: 42 })) of
  Rep r -> r.value
"#;
    let Some(output) = super::run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

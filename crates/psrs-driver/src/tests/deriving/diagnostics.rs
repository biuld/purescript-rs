/// A deriving failure carries the official `errorCode` for its condition, not
/// one catch-all kind. This is the diagnostic half of the deriving design; the
/// official differential battery compares acceptance, not codes.
#[test]
fn deriving_failures_report_their_official_error_codes() {
    let unknown_class = r#"module Main where

class Marker a

data Box = Box

derive instance markerBox :: Marker Box

main :: Int
main = 0
"#;
    let newtype_on_data = r#"module Main where

class ToInt a where
  toInt :: a -> Int

data Box = Box Int

derive newtype instance toIntBox :: ToInt Box

main :: Int
main = 0
"#;
    let functor_module = r#"module Data.Functor where

class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b
"#;
    let functor_contravariant = r#"module Main where

import Data.Functor (class Functor)

data Contra a = Contra (a -> Int)

derive instance functorContra :: Functor Contra

main :: Int
main = 0
"#;
    let newtype_module = r#"module Data.Newtype where

class Newtype t a
"#;
    let explicit_newtype_argument = r#"module Main where

import Data.Newtype (class Newtype)

newtype Age = Age Int

derive instance newtypeAge :: Newtype Age Int

main :: Int
main = 0
"#;
    let newtype_class_on_data = r#"module Main where

import Data.Newtype (class Newtype)

data Box = Box Int

derive instance newtypeBox :: Newtype Box _

main :: Int
main = 0
"#;
    let missing_mapping_instance_module = r#"module Data.Functor where

class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b
"#;
    let missing_mapping_instance = r#"module Main where

import Data.Functor (class Functor)

data Maybe a = Nothing | Just a

data Box a = Box (Maybe a)

derive instance functorBox :: Functor Box

main :: Int
main = 0
"#;
    fn assert_code(name: &str, expected: &str, sources: &[(&str, &str)]) {
        let errors = crate::check_program(sources).expect_err("deriving should be rejected");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.code == Some(expected)),
            "`{name}`: expected {expected}, got {errors:?}"
        );
    }

    assert_code(
        "unknown class has no rule",
        "CannotDerive",
        &[("Main.purs", unknown_class)],
    );
    assert_code(
        "derive newtype on a data type",
        "InvalidNewtypeInstance",
        &[("Main.purs", newtype_on_data)],
    );
    assert_code(
        "Newtype class on a data type",
        "CannotDeriveNewtypeForData",
        &[
            ("Data.Newtype.purs", newtype_module),
            ("Main.purs", newtype_class_on_data),
        ],
    );
    assert_code(
        "Newtype without a wildcard",
        "ExpectedWildcard",
        &[
            ("Data.Newtype.purs", newtype_module),
            ("Main.purs", explicit_newtype_argument),
        ],
    );
    assert_code(
        "field head with no mapping instance",
        "CannotDeriveInvalidConstructorArg",
        &[
            ("Data.Functor.purs", missing_mapping_instance_module),
            ("Main.purs", missing_mapping_instance),
        ],
    );
    assert_code(
        "Functor with a contravariant field",
        "CannotDeriveInvalidConstructorArg",
        &[
            ("Data.Functor.purs", functor_module),
            ("Main.purs", functor_contravariant),
        ],
    );
}

#[test]
fn fold_deriving_reports_missing_core_values_at_the_declaration() {
    for (module, class, method, parameters, fields) in [
        ("Data.Foldable", "Foldable", "foldMap", "a", "a a"),
        ("Data.Bifoldable", "Bifoldable", "bifoldMap", "a b", "a b"),
        ("Data.Foldable", "Foldable", "foldMap", "a", ""),
        ("Data.Bifoldable", "Bifoldable", "bifoldMap", "a b", ""),
    ] {
        let signature = if class == "Foldable" {
            "forall a m. (a -> m) -> t a -> m"
        } else {
            "forall a b m. (a -> m) -> (b -> m) -> t a b -> m"
        };
        let library =
            format!("module {module} where\nclass {class} t where\n  {method} :: {signature}\n");
        let main = format!(
            "module Main where\nimport {module}\ndata Box {parameters} = Box {fields}\nderive instance foldBox :: {class} Box\nmain = 0\n"
        );
        let errors = crate::check_program(&[("Fold.purs", &library), ("Main.purs", &main)])
            .expect_err("missing fold operations must reject deriving");
        let error = errors
            .iter()
            .find(|error| error.diagnostic.code == Some("CannotFindDerivingType"))
            .unwrap_or_else(|| panic!("{class}, fields `{fields}`: {errors:?}"));
        let span = error.diagnostic.span;
        let declaration = &main[span.start as usize..span.end as usize];
        assert!(
            declaration.contains("derive instance foldBox"),
            "{declaration}"
        );
        let operation = if fields.is_empty() {
            "mempty"
        } else {
            "append"
        };
        assert!(error.diagnostic.message.contains(operation));
    }
}

#[test]
fn deriving_rejects_non_constructor_heads_and_invalid_class_arity() {
    for (library, main, expected) in [
        (
            "module Data.Eq where\nclass Eq a where\n  eq :: a -> a -> Boolean\n",
            "module Main where\nimport Data.Eq\nderive instance eqInt :: Eq Int\nmain = 0\n",
            "ExpectedTypeConstructor",
        ),
        (
            "module Data.Eq where\nclass Eq a b where\n  eq :: a -> b -> Boolean\n",
            "module Main where\nimport Data.Eq\ndata Box = Box\nderive instance eqBox :: Eq Box Box\nmain = 0\n",
            "InvalidDerivedInstance",
        ),
    ] {
        let errors = crate::check_program(&[("Data.Eq.purs", library), ("Main.purs", main)])
            .expect_err("invalid deriving head should be rejected");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.code == Some(expected)),
            "{errors:?}"
        );
    }
}

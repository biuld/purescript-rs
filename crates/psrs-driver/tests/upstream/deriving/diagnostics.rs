use super::*;

#[test]
fn differential_deriving_error_codes_against_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
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
    let eq_builtin = "module Data.Eq where\nclass Eq a where\n  eq :: a -> a -> Boolean\nderive instance eqInt :: Eq Int\n";
    let eq_binary = "module Data.Eq where\nclass Eq a b where\n  eq :: a -> b -> Boolean\n";
    let invalid_arity = "module Main where\nimport Data.Eq\ndata Box = Box\nderive instance eqBox :: Eq Box Box\nmain = 0\n";
    let class_arity = "module Main where\nimport Data.Functor\ndata Box a = Box a\nderive instance functorBox :: Functor Box Int\nmain = 0\n";
    let cases: [(&str, &[(&str, &str)]); 6] = [
        (
            "functor-contravariant-field",
            &[
                ("Data.Functor.purs", functor_module),
                ("Main.purs", functor_contravariant),
            ],
        ),
        ("builtin-head", &[("Data.Eq.purs", eq_builtin)]),
        (
            "invalid-class-arity",
            &[("Data.Eq.purs", eq_binary), ("Main.purs", invalid_arity)],
        ),
        (
            "class-instance-arity",
            &[
                ("Data.Functor.purs", functor_module),
                ("Main.purs", class_arity),
            ],
        ),
        ("unknown-class", &[("Main.purs", unknown_class)]),
        ("newtype-on-data", &[("Main.purs", newtype_on_data)]),
    ];
    let mut failures = Vec::new();
    for (name, sources) in cases {
        let purs_output = purs_sources_output(name, sources);
        let purs_codes = purs_error_codes(&purs_output);
        let psrs_codes: Vec<String> = match psrs_driver::check_program(sources) {
            Ok(()) => Vec::new(),
            Err(errors) => errors
                .into_iter()
                .filter_map(|error| error.diagnostic.code.map(str::to_owned))
                .collect(),
        };
        if psrs_codes.is_empty() {
            failures.push(format!("`{name}`: psrs reported no errorCode"));
        }
        for code in &psrs_codes {
            if !purs_codes.contains(code) {
                failures.push(format!(
                    "`{name}`: psrs code `{code}` not among purs codes {purs_codes:?}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "deriving error-code differential failures:\n{}",
        failures.join("\n")
    );
}

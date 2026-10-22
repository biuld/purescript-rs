pub fn cases() -> Vec<(&'static str, String)> {
    let declarations = r#"module Main where
data Proxy a = Proxy
class Reflect a b | a -> b where
  reflectValue :: Proxy a -> b
instance Reflect Int Int where
  reflectValue _ = 42
reflect :: forall @a b. Reflect a b => b
reflect = reflectValue (Proxy @a)
plain :: forall b. Reflect Int b => b
plain = reflect @Int
class Collect a where
  collect :: { value :: a } -> Int
instance Collect Int where
  collect record = record.value
"#;
    [
        ("visible-field", "main = collect { value: reflect @Int }"),
        (
            "visible-update",
            "main = collect ({ value: 0 } { value = reflect @Int })",
        ),
        ("plain-field", "main = collect { value: plain }"),
        (
            "open-record-field",
            "identityRecord :: forall row. { | row } -> { | row }\nidentityRecord record = record\nopen = identityRecord { value: reflect @Int }\nmain = collect open",
        ),
        (
            "inferred-array-element",
            "values = [reflect @Int]\nmain = case values of\n  [42] -> 42\n  _ -> 1",
        ),
        (
            "local-visible-field",
            "main = let local :: forall @a b. Reflect a b => b\n           local = reflect @a\n       in collect { value: local @Int }",
        ),
    ]
    .into_iter()
    .map(|(name, body)| (name, format!("{declarations}\n{body}\n")))
    .chain(std::iter::once((
        "annotated-polymorphic-field",
        r#"module Main where
record = { identity: ((\x -> x) :: forall a. a -> a) }
first :: { value :: Int }
first = record.identity { value: 42 }
main :: Int
main = record.identity first.value
"#
        .to_owned(),
    )))
    .collect()
}

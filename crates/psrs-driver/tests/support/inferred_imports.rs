//! Checked inferred schemes must survive direct and transitive import boundaries.

pub const ACCEPT: &[(&str, &[(&str, &str)])] = &[
    (
        "visible_binder",
        &[
            (
                "Provider.purs",
                r#"module Provider where
identity :: forall @a. a -> a
identity value = value
"#,
            ),
            (
                "Main.purs",
                "module Main where\nimport Provider (identity)\nmain :: Int\nmain = identity @Int 42\n",
            ),
        ],
    ),
    (
        "transitive_values",
        &[
            (
                "Provider.purs",
                "module Provider where\nidentity value = value\n",
            ),
            (
                "Bridge.purs",
                "module Bridge where\nimport Provider as P\nalias = P.identity\n",
            ),
            (
                "Main.purs",
                "module Main where\nimport Bridge (alias)\nmain :: Int\nmain = if alias true then alias 42 else 1\n",
            ),
        ],
    ),
    (
        "ordered_constraints",
        &[
            (
                "Provider.purs",
                r#"module Provider where
class First a where
  first :: a -> Int
class Second a where
  second :: a -> Boolean
instance firstInt :: First Int where
  first value = value
instance secondBoolean :: Second Boolean where
  second value = value
inferred x y = if second y then first x else 1
"#,
            ),
            (
                "Main.purs",
                "module Main where\nimport Provider (inferred)\nmain :: Int\nmain = inferred 42 true\n",
            ),
        ],
    ),
    (
        "qualified_reexport",
        &[
            (
                "Provider.purs",
                r#"module Provider where
identity :: forall a. a -> a
identity value = value
alias = identity
answer = 42
"#,
            ),
            (
                "Umbrella.purs",
                "module Umbrella (module Provider) where\nimport Provider\n",
            ),
            (
                "Main.purs",
                r#"module Main where
import Umbrella as U
main :: Int
main = if U.alias true then U.alias U.answer else 1
"#,
            ),
        ],
    ),
    (
        "constrained_value",
        &[
            (
                "Provider.purs",
                r#"module Provider where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
instance markBoolean :: Mark Boolean where
  mark flag = if flag then 42 else 1
inferred value = mark value
"#,
            ),
            (
                "Main.purs",
                r#"module Main where
import Provider (inferred)
main :: Int
main = case inferred true of
  42 -> inferred 42
  _ -> 1
"#,
            ),
        ],
    ),
    (
        "nested_forall",
        &[
            (
                "Provider.purs",
                r#"module Provider where
box :: { run :: forall a. a -> a }
box = { run: \value -> value }
inferred = box
"#,
            ),
            (
                "Main.purs",
                r#"module Main where
import Provider (inferred)
main :: Int
main = if inferred.run true then inferred.run 42 else 1
"#,
            ),
        ],
    ),
    (
        "polykinded_value",
        &[
            (
                "Provider.purs",
                r#"module Provider where
data Proxy :: forall k. k -> Type
data Proxy a = Proxy
inferred = Proxy
"#,
            ),
            (
                "Main.purs",
                r#"module Main where
import Provider (Proxy(..), inferred)
symbol :: Proxy "hello"
symbol = inferred
higher :: Proxy Array
higher = inferred
main :: Int
main = case symbol, higher of
  Proxy, Proxy -> 42
"#,
            ),
        ],
    ),
];

pub const MISSING: &[(&str, &str)] = &[
    (
        "Provider.purs",
        r#"module Provider where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
inferred value = mark value
"#,
    ),
    (
        "Main.purs",
        "module Main where\nimport Provider (inferred)\nmain :: Int\nmain = inferred true\n",
    ),
];

pub const BAD_PRODUCER: &[(&str, &str)] = &[
    (
        "Provider.purs",
        "module Provider where\nbad :: Int\nbad = true\n",
    ),
    (
        "Main.purs",
        "module Main where\nimport Provider (bad)\nmain :: Int\nmain = bad\n",
    ),
];

pub const PRIVATE: &[(&str, &str)] = &[
    (
        "Provider.purs",
        "module Provider (public) where\npublic = 42\nprivate = 1\n",
    ),
    (
        "Main.purs",
        "module Main where\nimport Provider (private)\nmain :: Int\nmain = private\n",
    ),
];

pub const FOREIGN: &[(&str, &str)] = &[
    (
        "Provider.purs",
        "module Provider where\nforeign import identityForeign :: forall a. a -> a\n",
    ),
    (
        "Main.purs",
        "module Main where\nimport Provider (identityForeign)\nmain :: Int\nmain = if identityForeign true then identityForeign 42 else 1\n",
    ),
];

pub const CYCLE: &[(&str, &str)] = &[
    ("A.purs", "module A where\nimport B (b)\na = b\n"),
    ("B.purs", "module B where\nimport A (a)\nb = a\n"),
];

pub const LIB: &str = include_str!("../../../../tests/upstream/passing/4105/Lib.purs");

pub const MAIN: &str = r#"module Main where
import Lib (Patch)
newtype Wrapper = Wrapper Patch
main :: Int
main = case Wrapper { bio: "Done" } of
  Wrapper { bio: "Done" } -> 42
  _ -> 1
"#;

pub const ROW_SYMBOL: &str = r#"module Main where
data Proxy :: forall k. k -> Type
data Proxy a = Proxy
type Allowed = Proxy (bio :: "symbol")
main :: Int
main = 42
"#;

pub const REJECT: &[(&str, &str)] = &[
    (
        "record-shorthand",
        "module Main where\ntype Bad = { bio :: \"symbol\" }\n",
    ),
    (
        "record-application",
        "module Main where\ntype Bad = Record (bio :: \"symbol\")\n",
    ),
    (
        "record-tail",
        "module Main where\ntype Bad (r :: Row Symbol) = { bio :: String | r }\n",
    ),
];

pub fn sources(main: &str) -> Vec<(&str, &str)> {
    vec![("Lib.purs", LIB), ("Main.purs", main)]
}

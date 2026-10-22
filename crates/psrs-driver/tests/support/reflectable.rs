pub const PROXY: &str =
    "module Type.Proxy where\ndata Proxy :: forall k. k -> Type\ndata Proxy a = Proxy\n";
pub const ORDERING: &str = "module Data.Ordering where\ndata Ordering = LT | EQ | GT\n";
pub const REFLECTABLE: &str = r#"module Data.Reflectable where
import Type.Proxy (Proxy)
class Reflectable :: forall k. k -> Type -> Constraint
class Reflectable v t | v -> t where
  reflectType :: Proxy v -> t
"#;

pub fn inputs(main: &str) -> Vec<(&str, &str)> {
    vec![
        ("Proxy.purs", PROXY),
        ("Ordering.purs", ORDERING),
        ("Reflectable.purs", REFLECTABLE),
        ("Main.purs", main),
    ]
}

pub fn cases() -> Vec<(&'static str, String)> {
    [
        ("symbol", "\"λ😀\"", "\"λ😀\""),
        ("positive-int", "42", "42"),
        ("negative-int", "(-42)", "(-42)"),
        ("maximum-int", "2147483647", "2147483647"),
        ("minimum-int", "(-2147483648)", "(-2147483648)"),
        ("true", "B.True", "true"),
        ("false", "B.False", "false"),
        ("less", "O.LT", "V.LT"),
        ("equal", "O.EQ", "V.EQ"),
        ("greater", "O.GT", "V.GT"),
    ]
    .into_iter()
    .map(|(name, ty, pattern)| {
        (
            name,
            format!(
                "module Main where\n\
                 import Data.Reflectable as R\n\
                 import Type.Proxy (Proxy(..))\n\
                 import Prim.Boolean as B\n\
                 import Prim.Ordering as O\n\
                 import Data.Ordering as V\n\
                 reflected = R.reflectType (Proxy :: Proxy {ty})\n\
                 main :: Int\n\
                 main = case reflected of\n  {pattern} -> 42\n  _ -> 1\n"
            ),
        )
    })
    .collect()
}

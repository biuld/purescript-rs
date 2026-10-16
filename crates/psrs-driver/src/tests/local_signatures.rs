//! An explicit polymorphic annotation on a local binding is predeclared at its
//! written type, so a recursive use instantiates the annotation rather than
//! binding a fresh metavariable to an arrow before the quantifier is checked.

#[test]
fn a_recursive_local_signature_keeps_its_quantifiers() {
    let source = r#"module Main where

import Prelude

data List a = Nil | Cons a (List a)

foldlLocal :: forall b c. (c -> b -> c) -> c -> List b -> c
foldlLocal k c xs = go k c xs
  where
  go :: forall b2 c2. (c2 -> b2 -> c2) -> c2 -> List b2 -> c2
  go _ acc Nil = acc
  go k2 acc (Cons b rest) = go k2 (k2 acc b) rest

main :: Int
main = foldlLocal (\acc x -> acc + x) 0 (Cons 1 (Cons 2 (Cons 3 Nil)))
"#;
    let Some(output) = super::run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(6), "{output:?}");
}

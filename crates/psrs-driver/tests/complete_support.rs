use std::process::Command;

const SOURCE: &str = r#"module Main where
import Prelude
import Data.Foldable (class Foldable, foldl, foldr, foldMap)
import Data.Traversable (class Traversable, traverse)
import Data.Tuple (Tuple(..))
import Data.Predicate (Predicate(..))
import Data.Functor.Contravariant (class Contravariant, cmap)
import Data.Profunctor (class Profunctor, dimap)

data Covariant a = Covariant (Tuple a Int) (Tuple a a)
derive instance Functor Covariant
derive instance Foldable Covariant
derive instance Traversable Covariant

data Contra a = Contra (a -> Int) (Tuple (Predicate a) Int)
derive instance Contravariant Contra

data Pro a b = Pro (a -> b) (Tuple b Int)
derive instance Profunctor Pro

values :: Covariant Int
values = Covariant (Tuple 1 99) (Tuple 2 3)

covariantOK :: Boolean
covariantOK =
  foldl (\acc n -> acc * 10 + n) 0 values == 123 &&
  foldr (\n acc -> n + 10 * acc) 0 values == 321 &&
  foldMap (\n -> [n]) values == [1, 2, 3] &&
  (case map (\n -> n + 10) values of
    Covariant (Tuple a fixed) (Tuple b c) ->
      a == 11 && b == 12 && c == 13 && fixed == 99) &&
  (case traverse (\n -> Tuple [n] (n + 1)) values of
    Tuple order (Covariant (Tuple a fixed) (Tuple b c)) ->
      order == [1, 2, 3] && a == 2 && b == 3 && c == 4 && fixed == 99)

contraOK :: Boolean
contraOK = case cmap (\n -> n + 1) (Contra (\n -> n * 2) (Tuple (Predicate (\n -> n == 4)) 77)) of
  Contra f (Tuple (Predicate p) fixed) ->
    f 3 == 8 && p 3 && not (p 2) && fixed == 77

proOK :: Boolean
proOK = case dimap (\n -> n + 1) (\n -> n * 2) (Pro (\n -> n * 3) (Tuple 5 88)) of
  Pro f (Tuple b fixed) -> f 2 == 18 && b == 10 && fixed == 88

main :: Int
main = if covariantOK && contraOK && proOK then 42 else 1
"#;

/// Checks the complete locked inventory and executes all generated methods.
/// Run explicitly in release mode alongside the runtime scoreboard.
#[test]
#[ignore = "complete locked support inventory; execute explicitly with required Wasmtime"]
fn complete_locked_support_preserves_derived_values_and_callback_order() {
    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    assert!(
        available,
        "this acceptance test requires actual Wasmtime execution"
    );
    let sources = [("Main.purs", SOURCE)];
    let errors = psrs_driver::compile_program_sources_with_prelude(&sources)
        .expect_err("ordinary imports lack the bipartite support classes");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("CannotDeriveInvalidConstructorArg")),
        "{errors:?}"
    );
    let artifact = psrs_driver::compile_program_sources_with_complete_support(&sources)
        .unwrap_or_else(|errors| panic!("complete support: {errors:?}"));
    let path =
        std::env::temp_dir().join(format!("psrs-locked-support-{}.wasm", std::process::id()));
    std::fs::write(&path, artifact.wasm).unwrap();
    let output = Command::new("wasmtime")
        .args(["run", "-W", "max-wasm-stack=524288"])
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
}

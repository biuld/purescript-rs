use super::*;

#[test]
fn retired_arithmetic_bindings_are_rejected() {
    for name in ["intDiv", "intMod"] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#{name}\" old :: Int -> Int -> Int\nmain = old 3 (-2)\n"
        );
        let errors =
            crate::check_program(&[("Main.purs", source.as_str())]).expect_err("retired binding");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.message.contains(&format!("`{name}`"))),
            "{errors:?}"
        );
        let source = format!("module Main where\nmain = {name} 3 2\n");
        assert!(crate::check_program(&[("Main.purs", source.as_str())]).is_err());
    }
}

#[test]
fn official_euclidean_policy_executes_for_both_signs_and_zero() {
    let source = r#"module Main where
import Prelude
import Data.EuclideanRing as E

check a b q r = E.div a b == q && E.mod a b == r
checkHigher f a b expected = f a b == expected
main = if check 5 3 1 2
  && check (-5) 3 (-2) 1
  && check 5 (-3) (-1) 2
  && check (-5) (-3) 2 1
  && check 3 (-2) (-1) 1
  && check (-3) 2 (-2) 1
  && check 6 (-3) (-2) 0
  && check (-6) (-3) 2 0
  && check 1 0 0 0 && check (-1) 0 0 0 && check 0 0 0 0
  && checkHigher E.div 3 (-2) (-1)
  && checkHigher E.mod 3 (-2) 1
  then 42 else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn balanced_public_array_traversal_uses_the_truncating_storage_leaf() {
    let source = r#"module Main where
import Prelude
import Data.Maybe (Maybe(..))
import Data.Traversable (traverse)

large = case traverse (\x -> Just (x + 1)) [1,2,3,4,5,6,7,8,9] of
  Just xs -> arrayLength xs == 9 && arrayIndex xs 0 == 2
    && arrayIndex xs 4 == 6 && arrayIndex xs 8 == 10
  Nothing -> false
empty = case traverse (\x -> Just (x + 1)) [] of
  Just xs -> arrayLength xs == 0
  Nothing -> false
failure = case traverse (\x -> if x == 5 then Nothing else Just x) [1,2,3,4,5,6,7,8,9] of
  Nothing -> true
  Just _ -> false
main = if large && empty && failure then 42 else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
}

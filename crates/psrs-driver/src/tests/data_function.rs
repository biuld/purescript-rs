//! The library's function-application surface (`Data.Function` / `Prelude`).
//!
//! `$` and `#` are value-operator aliases, so these tests check more than the
//! functions' values: they check that an alias defined in one library module
//! reaches a corpus module through `Prelude`'s selective re-export and keeps
//! its target identity and fixity while it does.

use super::*;

#[test]
fn the_application_operators_resolve_through_the_library_re_export() {
    let source = r#"
module Main where

import Prelude
import Effect (Effect)
import Effect.Console (log)
import Data.Function (applyFlipped, const, flip)

double :: Int -> Int
double x = x + x

viaDollar :: Int
viaDollar = double $ 20 + 1

viaHash :: Int
viaHash = 21 # double

viaApplyFlipped :: Int
viaApplyFlipped = applyFlipped 42 double

viaFlip :: Int -> Int -> Int
viaFlip = flip (\a b -> a + b)

viaConst :: Int
viaConst = const 1 2

checks :: Effect Unit
checks = do
  log (if viaDollar == 42 then "dollar" else "dollar-wrong")
  log (if viaHash == 42 then "hash" else "hash-wrong")
  log (if viaApplyFlipped == 84 then "flipped" else "flipped-wrong")
  log (if viaFlip 1 2 == 3 then "flip" else "flip-wrong")
  log (if viaConst == 1 then "const" else "const-wrong")
  pure unit

main = let ignored = runEffect checks in 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        output.stdout, b"dollar\nhash\nflipped\nflip\nconst\n",
        "`$`, `#`, and the Data.Function values must reach the same results: {output:?}"
    );
}

#[test]
fn dollar_is_right_associative_and_lowest_precedence() {
    // `$` is `infixr 0`, so `f $ g $ x` groups as `f $ (g $ x)` and `f $ x + y`
    // groups as `f $ (x + y)`. A left-associative or higher-precedence alias
    // would fail to type or compute a different result.
    let source = r#"
module Main where

import Prelude

addOne :: Int -> Int
addOne x = x + 1

main = if (addOne $ addOne $ addOne 39) == 42 then 40 + (addOne $ 2 * 3) - 6 else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    // `addOne $ 2 * 3` is `(addOne 2) * 3` only if `$` bound tighter than `*`,
    // which would make the result 40 + 9 - 6 = 43. `$` is `infixr 0`, so it is
    // `addOne (2 * 3)` and the result is 40 + 7 - 6 = 41.
    assert_eq!(output.status.code(), Some(41), "{output:?}");
}

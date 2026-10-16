#[test]
fn public_mutable_arrays_copy_safe_conversions_and_share_growth_through_aliases() {
    let source = r#"module Main where
import Prelude
import Control.Monad.ST as ST
import Data.Array.ST as A
import Data.Array.ST.Partial as PartialArray
import Data.Maybe (Maybe(..))
import Effect.Unsafe (unsafePerformEffect)
import WASI.Console (log)
import Partial.Unsafe (unsafePartial)

update :: forall r a. a -> A.STArray r a -> ST.ST r Boolean
update value xs = A.poke 0 value xs

main :: Int
main = ST.run do
  let original = [1, 2]
  xs <- A.thaw original
  let alias = xs
  _ <- update 7 xs
  frozen <- A.freeze xs
  copied <- A.clone xs
  _ <- update 8 alias
  _ <- A.push 9 xs
  grown <- A.length alias
  current <- A.peek 0 alias
  cloneFirst <- A.peek 0 copied
  cloneLength <- A.length copied
  _ <- A.modify 1 (\value ->
    let _ = unsafePerformEffect (log "array-callback") in value + 10) alias
  second <- A.peek 1 xs
  text <- A.thaw ["first"]
  _ <- update "second" text
  changed <- A.peek 0 text
  partialFirst <- unsafePartial (PartialArray.peek 0 xs)
  _ <- unsafePartial (PartialArray.poke 2 10 xs)
  partialThird <- A.peek 2 xs
  owned <- A.unsafeThaw [4, 5]
  _ <- update 6 owned
  transferred <- A.unsafeFreeze owned
  if arrayIndex original 0 == 1 && arrayIndex frozen 0 == 7
    && grown == 3 && current == Just 8 && cloneFirst == Just 7
    && cloneLength == 2 && second == Just 12 && changed == Just "second"
    && partialFirst == 8 && partialThird == Just 10
    && arrayIndex transferred 0 == 6
    then pure 42 else pure 0
"#;
    let Some(output) = super::super::run_program_with_prelude_wasmtime(&[("Main.purs", source)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert_eq!(output.stdout, b"array-callback\n", "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

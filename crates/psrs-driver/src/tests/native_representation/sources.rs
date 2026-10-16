//! Official Effect/ST programs over the package native representation.

pub const ENTRY: &str = r#"module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Global (toEffect)
import Control.Monad.ST.Ref as STRef
import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)

storage :: forall r. ST.ST r Int
storage = do
  ints <- STRef.new 0
  strs <- STRef.new "a"
  _ <- STRef.write 41 ints
  _ <- STRef.write "b" strs
  first <- STRef.read ints
  second <- STRef.read strs
  if second == "b" then pure (first + 1) else pure 0

effectProgram :: Effect Int
effectProgram = do
  ints <- Ref.new 0
  strs <- Ref.new "a"
  _ <- Ref.write 41 ints
  _ <- Ref.write "b" strs
  first <- Ref.read ints
  second <- Ref.read strs
  if second == "b" then pure (first + 1) else pure 0

freshRefs :: Effect Int
freshRefs = do
  a <- Ref.read r1
  b <- Ref.read r2
  if a == 1 && b == 2 then pure 42 else pure 0
  where
  r1 :: Ref Int
  r1 = unsafePerformEffect (do
    r <- Ref.new 0
    _ <- Ref.write 1 r
    pure r)
  r2 :: Ref Int
  r2 = unsafePerformEffect (do
    r <- Ref.new 0
    _ <- Ref.write 2 r
    pure r)

main :: Int
main =
  let viaST = ST.run storage
      viaSTAgain = ST.run storage
      viaEffect = unsafePerformEffect effectProgram
      viaGlobal = unsafePerformEffect (toEffect storage)
      viaFresh = unsafePerformEffect freshRefs
  in if viaST == 42 && viaSTAgain == 42 && viaEffect == 42 && viaGlobal == 42 && viaFresh == 42
    then 42
    else 0
"#;

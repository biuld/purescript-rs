module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)
import WASI.Console (log)

readAny :: forall a. Ref.Ref a -> Effect a
readAny = Ref.read

writeAny :: forall a. a -> Ref.Ref a -> Effect Unit
writeAny = Ref.write

rank :: Effect Int
rank = do
  int <- Ref.new 41
  text <- Ref.new "boxed"
  record <- Ref.new { value: 0 }
  a <- readAny int
  b <- readAny text
  _ <- writeAny { value: a + 1 } record
  seen <- readAny record
  _ <- writeAny "stored" text
  again <- readAny text
  if b == "boxed" && seen.value == 42 && again == "stored" then pure 42 else pure 0

share :: Effect Boolean
share = do
  cell <- Ref.new [1]
  let alias = cell
  _ <- writeAny [2, 3] cell
  read <- readAny alias
  pure (arrayLength read == 2)

order :: Effect Int
order = do
  cell <- Ref.new 0
  log "before"
  _ <- writeAny 7 cell
  log "after"
  readAny cell

use :: forall r. STRef.STRef r Int -> ST.ST r Int
use = STRef.read

stRank :: Int
stRank = ST.run do
  ref <- STRef.new 20
  value <- use ref
  pure (value + 1)

main :: Int
main = unsafePerformEffect do
  ranked <- rank
  shared <- share
  ordered <- order
  if ranked == 42 && shared && ordered == 7 && stRank == 21 then pure 42 else pure 0

module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)
import Unsafe.Coerce (unsafeCoerce)
import WASI.Console (log)

readBad :: Effect Int
readBad = Ref.read (unsafeCoerce 1 :: Ref.Ref Int)

main :: Int
main = unsafePerformEffect do
  value <- readBad
  log "reached"
  pure value

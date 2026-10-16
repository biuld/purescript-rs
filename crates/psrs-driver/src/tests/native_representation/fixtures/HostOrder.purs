module Main where

import Prelude

import Effect (Effect)
import Effect.Unsafe (unsafePerformEffect)
import WASI.Clock (now)
import WASI.Console (log)
import WASI.Process (arguments)
import WASI.Random (randomBytes)

run :: Effect Int
run = do
  log "first"
  names <- arguments
  log "second"
  bytes <- randomBytes 2
  tick <- now
  if arrayLength names >= 0 && arrayLength bytes == 2 && tick * 0 == 0 then pure 42 else pure 0

main :: Int
main =
  let stored = log "stored"
      _ = stored
  in unsafePerformEffect run

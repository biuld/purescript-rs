module Main where

import Prelude
import WASI.Console

main = let ignored = runEffect (log "hello world") in 0

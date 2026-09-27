module WASI.Stdin (getStdin) where

import Prelude
import WASI.Streams (InputStream)

foreign import "wasi:cli/stdin#get-stdin" getStdinRaw :: InputStream

getStdin :: Effect InputStream
getStdin = \token -> getStdinRaw

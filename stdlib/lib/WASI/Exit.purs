module WASI.Exit (exitWithCode) where

import Prelude

foreign import "wasi:cli/exit#exit-with-code" exitWithCodeRaw :: Int -> Unit

exitWithCode :: Int -> Effect Unit
exitWithCode code = \token -> exitWithCodeRaw code

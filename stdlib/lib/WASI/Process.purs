-- | Idiomatic wrappers over `wasi:cli`: process exit and the command-line
-- | arguments and environment. This module merges the former `WASI.Exit` and
-- | `WASI.Environment`.
module WASI.Process (exitWithCode, arguments, environment) where

import Prelude

foreign import "wasi:cli/exit#exit-with-code" exitWithCodeRaw :: Int -> Unit
foreign import "wasi:cli/environment#get-arguments" getArguments :: Array String
foreign import "wasi:cli/environment#get-environment" getEnvironment :: Array { _1 :: String, _2 :: String }

exitWithCode :: Int -> Effect Unit
exitWithCode code = \token -> exitWithCodeRaw code

arguments :: Effect (Array String)
arguments = \token -> getArguments

-- | The environment variables, each a `(name, value)` pair. The canonical ABI
-- | fixes a tuple's field names to `_1`/`_2`.
environment :: Effect (Array { _1 :: String, _2 :: String })
environment = \token -> getEnvironment

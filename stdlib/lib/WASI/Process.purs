-- | Idiomatic wrappers over `wasi:cli`: process exit and the command-line
-- | arguments and environment. This module merges the former `WASI.Exit` and
-- | `WASI.Environment`.
module WASI.Process (exitWithCode, arguments, environment) where

import Prelude

foreign import "wasi:cli/exit#exit-with-code" exitWithCode :: Int -> Effect (Unit)
foreign import "wasi:cli/environment#get-arguments" arguments :: Effect (Array String)
foreign import "wasi:cli/environment#get-environment" environment :: Effect (Array { _1 :: String, _2 :: String })

-- | The environment variables, each a `(name, value)` pair. The canonical ABI
-- | fixes a tuple's field names to `_1`/`_2`.

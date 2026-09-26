module WASI.Environment (arguments) where

import Prelude

foreign import "wasi:cli/environment#get-arguments" getArguments :: Array String

arguments :: Effect (Array String)
arguments = \token -> getArguments

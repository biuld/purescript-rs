module WASI.Error (toDebugString) where

import Prelude
import WASI.Streams (Error)

foreign import "wasi:io/error#[method]error.to-debug-string" toDebugStringRaw :: Error -> String

toDebugString :: Error -> Effect String
toDebugString err = \token -> toDebugStringRaw err


module WASI.Clock where

import Prelude

foreign import "wasi:clocks/monotonic-clock#now" monotonicNow :: Int

now :: Effect Int
now = \token -> monotonicNow

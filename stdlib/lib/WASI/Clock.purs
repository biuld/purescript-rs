
module WASI.Clock (now) where

import Prelude

foreign import "wasi:clocks/monotonic-clock#now" monotonicNow :: Int

now :: Effect Int
now = \token -> monotonicNow

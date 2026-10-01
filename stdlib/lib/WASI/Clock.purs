-- | Idiomatic wrappers over `wasi:clocks`: the monotonic and wall clocks, and
-- | the two clock subscriptions that yield a `Resource Pollable`.
module WASI.Clock (now, wallNow, wallResolution, subscribeInstant, subscribeDuration) where

import Prelude
import WASI.IO (Pollable)
import WASI.Resource (Resource)

foreign import "wasi:clocks/monotonic-clock#now" now :: Effect (Int)
foreign import "wasi:clocks/wall-clock#now" wallNow :: Effect ({ seconds :: Int, nanoseconds :: Int })
foreign import "wasi:clocks/wall-clock#resolution" wallResolution :: Effect ({ seconds :: Int, nanoseconds :: Int })
foreign import "wasi:clocks/monotonic-clock#subscribe-instant" subscribeInstant :: Int -> Effect (Resource Pollable)
foreign import "wasi:clocks/monotonic-clock#subscribe-duration" subscribeDuration :: Int -> Effect (Resource Pollable)


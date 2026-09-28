-- | Idiomatic wrappers over `wasi:clocks`: the monotonic and wall clocks, and
-- | the two clock subscriptions that yield a `Resource Pollable`.
module WASI.Clock (now, wallNow, wallResolution, subscribeInstant, subscribeDuration) where

import Prelude
import WASI.IO (Pollable)
import WASI.Resource (Resource)

foreign import "wasi:clocks/monotonic-clock#now" monotonicNow :: Int
foreign import "wasi:clocks/wall-clock#now" wallNowRaw :: { seconds :: Int, nanoseconds :: Int }
foreign import "wasi:clocks/wall-clock#resolution" wallResolutionRaw :: { seconds :: Int, nanoseconds :: Int }
foreign import "wasi:clocks/monotonic-clock#subscribe-instant" subscribeInstantRaw :: Int -> Resource Pollable
foreign import "wasi:clocks/monotonic-clock#subscribe-duration" subscribeDurationRaw :: Int -> Resource Pollable

now :: Effect Int
now = \token -> monotonicNow

wallNow :: Effect { seconds :: Int, nanoseconds :: Int }
wallNow = \token -> wallNowRaw

wallResolution :: Effect { seconds :: Int, nanoseconds :: Int }
wallResolution = \token -> wallResolutionRaw

subscribeInstant :: Int -> Effect (Resource Pollable)
subscribeInstant when = \token -> subscribeInstantRaw when

subscribeDuration :: Int -> Effect (Resource Pollable)
subscribeDuration duration = \token -> subscribeDurationRaw duration

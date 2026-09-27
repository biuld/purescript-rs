module WASI.Clock (now, wallNow, wallResolution, subscribeInstant, subscribeDuration) where

import Prelude
import WASI.Streams (Pollable)

foreign import "wasi:clocks/monotonic-clock#now" monotonicNow :: Int
foreign import "wasi:clocks/wall-clock#now" wallNowRaw :: { seconds :: Int, nanoseconds :: Int }
foreign import "wasi:clocks/wall-clock#resolution" wallResolutionRaw :: { seconds :: Int, nanoseconds :: Int }
foreign import "wasi:clocks/monotonic-clock#subscribe-instant" subscribeInstantRaw :: Int -> Pollable
foreign import "wasi:clocks/monotonic-clock#subscribe-duration" subscribeDurationRaw :: Int -> Pollable

now :: Effect Int
now = \token -> monotonicNow

wallNow :: Effect { seconds :: Int, nanoseconds :: Int }
wallNow = \token -> wallNowRaw

wallResolution :: Effect { seconds :: Int, nanoseconds :: Int }
wallResolution = \token -> wallResolutionRaw

subscribeInstant :: Int -> Effect Pollable
subscribeInstant when = \token -> subscribeInstantRaw when

subscribeDuration :: Int -> Effect Pollable
subscribeDuration duration = \token -> subscribeDurationRaw duration

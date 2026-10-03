-- | The corpus's console surface, a thin binding over the platform layer
-- | `WASI.Console`
-- | ([DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md)).
-- |
-- | Nothing here chooses a stream or performs a write: `log` and `error` are the
-- | platform functions under their corpus names, and `warn` is the platform's.
-- | That keeps one place that decides where output goes, rather than a
-- | wrapper that could drift from it.
-- |
-- | `logShow` is **absent**, not approximated. Its official type is
-- | `forall a. Show a => a -> Effect Unit`, and no module in this library
-- | declares a `Show` class yet, so providing it would mean either inventing a
-- | second notion of stringification or special-casing the handful of types the
-- | corpus happens to print. Both are the narrower special case the iteration
-- | principles reject. It lands with the class surface in #94.
module Effect.Console (log, warn, error) where

import Prelude
import WASI.Console (error, log, warn)
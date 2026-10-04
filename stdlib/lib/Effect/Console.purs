-- | The corpus's console surface, a thin binding over the platform layer
-- | `WASI.Console`
-- | ([DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md)).
-- |
-- | Nothing here chooses a stream or performs a write: `log` and `error` are the
-- | platform functions under their corpus names, and `warn` is the platform's.
-- | That keeps one place that decides where output goes, rather than a
-- | wrapper that could drift from it.
-- |
-- | `logShow` is **absent**, not approximated. `Data.Show` declares the class
-- | it needs (`forall a. Show a => a -> Effect Unit` is `log` of `show`), but
-- | the wrapper is the remaining `Effect.Console` surface and stays with #95
-- | rather than being slipped in beside the class.
module Effect.Console (log, warn, error) where

import Prelude
import WASI.Console (error, log, warn)
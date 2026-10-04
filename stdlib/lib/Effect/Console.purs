-- | The corpus's console surface, a thin binding over the platform layer
-- | `WASI.Console`
-- | ([DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md)).
-- |
-- | Nothing here chooses a stream or performs a write: `log` and `error` are the
-- | platform functions under their corpus names, and `warn` is the platform's.
-- | That keeps one place that decides where output goes, rather than a
-- | wrapper that could drift from it.
-- |
-- | `logShow` adds no I/O of its own: it is `log` of `Data.Show.show`, so the
-- | rendering is the library's `Show` and the destination is still the one
-- | `WASI.Console.log` decides. It is a wrapper like `log`, not a second
-- | stringifier.
module Effect.Console (log, warn, error, logShow) where

import Prelude
import WASI.Console (error, log, warn)
import Data.Show (class Show, show)

-- | Writes the `Show` rendering of a value. `log` already writes the newline,
-- | so this is `log` composed with the library's `show`.
logShow :: forall a. Show a => a -> Effect Unit
logShow value = log (show value)

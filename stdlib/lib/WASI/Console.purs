-- | Convenience output over `WASI.IO`: write a line to standard output or
-- | standard error and drop the stream. A failed write is ignored so the
-- | public signature stays `String -> Effect Unit`.
-- |
-- | A WIT `list<u8>` is `Array Int`, not `String`, so the wrapper converts the
-- | message through the canonical UTF-8 bytes
-- | ([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
--
-- | `warn` joins `error` on the same stream. The platform layer owns the choice
-- | of stream so `Effect.Console` stays a thin binding over it rather than a
-- | second place that decides where a diagnostic goes
-- | ([DEC-11](../../../decision/DEC-11-primitive-ffi-stdlib-wrappers.md)).
module WASI.Console (log, warn, error) where

import Prelude
import WASI.IO (blockingWriteAndFlush, dropOutputStream, getStderr, getStdout)

log :: String -> Effect Unit
log s = do
  handle <- getStdout
  _ <- blockingWriteAndFlush handle (stringToBytes s)
  _ <- blockingWriteAndFlush handle (stringToBytes "\n")
  dropOutputStream handle

warn :: String -> Effect Unit
warn s = error s

error :: String -> Effect Unit
error s = do
  handle <- getStderr
  _ <- blockingWriteAndFlush handle (stringToBytes s)
  _ <- blockingWriteAndFlush handle (stringToBytes "\n")
  dropOutputStream handle

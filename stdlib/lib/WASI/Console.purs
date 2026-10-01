-- | Convenience output over `WASI.IO`: write a line to standard output or
-- | standard error and drop the stream. A failed write is ignored so the
-- | public signature stays `String -> Effect Unit`.
module WASI.Console (log, error) where

import Prelude
import WASI.IO (blockingWriteAndFlush, dropOutputStream, getStderr, getStdout)

log :: String -> Effect Unit
log s = do
  handle <- getStdout
  _ <- blockingWriteAndFlush handle s
  _ <- blockingWriteAndFlush handle "\n"
  dropOutputStream handle

error :: String -> Effect Unit
error s = do
  handle <- getStderr
  _ <- blockingWriteAndFlush handle s
  _ <- blockingWriteAndFlush handle "\n"
  dropOutputStream handle

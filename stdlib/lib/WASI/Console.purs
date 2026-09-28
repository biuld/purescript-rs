-- | Convenience output over `WASI.IO`: write a line to standard output or
-- | standard error and drop the stream. A failed write is ignored so the
-- | public signature stays `String -> Effect Unit`.
module WASI.Console (log, error) where

import Prelude
import WASI.IO (blockingWriteAndFlush, dropOutputStream, getStderr, getStdout)

log :: String -> Effect Unit
log s = \token ->
  let handle = getStdout token in
  let ignored = blockingWriteAndFlush handle s token in
  let ignoredNewline = blockingWriteAndFlush handle "\n" token in
  dropOutputStream handle token

error :: String -> Effect Unit
error s = \token ->
  let handle = getStderr token in
  let ignored = blockingWriteAndFlush handle s token in
  let ignoredNewline = blockingWriteAndFlush handle "\n" token in
  dropOutputStream handle token

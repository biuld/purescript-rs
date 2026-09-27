module WASI.Console (log, error) where

import Prelude
import Data.Either (Either)
import WASI.Streams (StreamError)

foreign import "wasi:cli/stdout#get-stdout" getStdout :: Int
foreign import "wasi:cli/stderr#get-stderr" getStderr :: Int
foreign import "wasi:io/streams#[resource-drop]output-stream" dropStreamRaw :: Int -> Unit
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush" writeStream :: Int -> String -> Either Unit StreamError

-- | Writes `s` and a trailing newline to stdout, then drops the handle. A
-- | failed write is ignored so the public signature stays `String -> Effect
-- | Unit`.
log :: String -> Effect Unit
log s = \token ->
  let handle = getStdout in
  let ignored = writeStream handle s in
  let ignoredNewline = writeStream handle "\n" in
  dropStreamRaw handle

-- | Writes `s` and a trailing newline to stderr, then drops the handle.
error :: String -> Effect Unit
error s = \token ->
  let handle = getStderr in
  let ignored = writeStream handle s in
  let ignoredNewline = writeStream handle "\n" in
  dropStreamRaw handle

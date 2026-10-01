-- | Idiomatic wrappers over `wasi:io`. This module merges the former
-- | `WASI.Streams`, `WASI.Poll`, `WASI.Error`, and `WASI.Stdin`: the streams,
-- | the pollable, and the error resource. Raw imports stay module private; the
-- | public API uses `Resource a` for every handle and `Data.Either` for every
-- | `result`, with the error on `Left`.
module WASI.IO
  ( InputStream
  , OutputStream
  , Pollable
  , Error
  , StreamError(..)
  , getStdin
  , getStdout
  , getStderr
  , read
  , blockingRead
  , skip
  , blockingSkip
  , write
  , blockingWriteAndFlush
  , flush
  , subscribeInputStream
  , subscribeOutputStream
  , toDebugString
  , poll
  , ready
  , block
  , dropInputStream
  , dropOutputStream
  , dropPollable
  , dropError
  ) where

import Prelude
import Data.Either (Either)
import WASI.Resource (Resource)

-- | Opaque phantom types naming the resources a `Resource` may wrap. They are
-- | not handle types themselves: every value is a `Resource X`.
foreign import data InputStream :: Type
foreign import data OutputStream :: Type
foreign import data Pollable :: Type
foreign import data Error :: Type

data StreamError = LastOperationFailed (Resource Error) | Closed

foreign import "wasi:cli/stdin#get-stdin" getStdin :: Effect (Resource InputStream)
foreign import "wasi:cli/stdout#get-stdout" getStdout :: Effect (Resource OutputStream)
foreign import "wasi:cli/stderr#get-stderr" getStderr :: Effect (Resource OutputStream)
foreign import "wasi:io/streams#[method]input-stream.read" read :: Resource InputStream -> Int -> Effect (Either StreamError (Array Int))
foreign import "wasi:io/streams#[method]input-stream.blocking-read" blockingRead :: Resource InputStream -> Int -> Effect (Either StreamError (Array Int))
foreign import "wasi:io/streams#[method]input-stream.skip" skip :: Resource InputStream -> Int -> Effect (Either StreamError Int)
foreign import "wasi:io/streams#[method]input-stream.blocking-skip" blockingSkip :: Resource InputStream -> Int -> Effect (Either StreamError Int)
foreign import "wasi:io/streams#[method]output-stream.write" write :: Resource OutputStream -> Array Int -> Effect (Either StreamError Unit)
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush" blockingWriteAndFlush :: Resource OutputStream -> Array Int -> Effect (Either StreamError Unit)
foreign import "wasi:io/streams#[method]output-stream.flush" flush :: Resource OutputStream -> Effect (Either StreamError Unit)
foreign import "wasi:io/streams#[method]input-stream.subscribe" subscribeInputStream :: Resource InputStream -> Effect (Resource Pollable)
foreign import "wasi:io/streams#[method]output-stream.subscribe" subscribeOutputStream :: Resource OutputStream -> Effect (Resource Pollable)
foreign import "wasi:io/error#[method]error.to-debug-string" toDebugString :: Resource Error -> Effect (String)
foreign import "wasi:io/poll#poll" poll :: Array (Resource Pollable) -> Effect (Array Int)
foreign import "wasi:io/poll#[method]pollable.ready" ready :: Resource Pollable -> Effect (Boolean)
foreign import "wasi:io/poll#[method]pollable.block" block :: Resource Pollable -> Effect (Unit)
foreign import "wasi:io/streams#[resource-drop]input-stream" dropInputStream :: Resource InputStream -> Effect (Unit)
foreign import "wasi:io/streams#[resource-drop]output-stream" dropOutputStream :: Resource OutputStream -> Effect (Unit)
foreign import "wasi:io/poll#[resource-drop]pollable" dropPollable :: Resource Pollable -> Effect (Unit)
foreign import "wasi:io/error#[resource-drop]error" dropError :: Resource Error -> Effect (Unit)


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

foreign import "wasi:cli/stdin#get-stdin" getStdinRaw :: Resource InputStream
foreign import "wasi:cli/stdout#get-stdout" getStdoutRaw :: Resource OutputStream
foreign import "wasi:cli/stderr#get-stderr" getStderrRaw :: Resource OutputStream
foreign import "wasi:io/streams#[method]input-stream.read" readRaw :: Resource InputStream -> Int -> Either StreamError String
foreign import "wasi:io/streams#[method]input-stream.blocking-read" blockingReadRaw :: Resource InputStream -> Int -> Either StreamError String
foreign import "wasi:io/streams#[method]input-stream.skip" skipRaw :: Resource InputStream -> Int -> Either StreamError Int
foreign import "wasi:io/streams#[method]input-stream.blocking-skip" blockingSkipRaw :: Resource InputStream -> Int -> Either StreamError Int
foreign import "wasi:io/streams#[method]output-stream.write" writeRaw :: Resource OutputStream -> String -> Either StreamError Unit
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush" blockingWriteAndFlushRaw :: Resource OutputStream -> String -> Either StreamError Unit
foreign import "wasi:io/streams#[method]output-stream.flush" flushRaw :: Resource OutputStream -> Either StreamError Unit
foreign import "wasi:io/streams#[method]input-stream.subscribe" subscribeInputStreamRaw :: Resource InputStream -> Resource Pollable
foreign import "wasi:io/streams#[method]output-stream.subscribe" subscribeOutputStreamRaw :: Resource OutputStream -> Resource Pollable
foreign import "wasi:io/error#[method]error.to-debug-string" toDebugStringRaw :: Resource Error -> String
foreign import "wasi:io/poll#poll" pollRaw :: Array (Resource Pollable) -> Array Int
foreign import "wasi:io/poll#[method]pollable.ready" readyRaw :: Resource Pollable -> Boolean
foreign import "wasi:io/poll#[method]pollable.block" blockRaw :: Resource Pollable -> Unit
foreign import "wasi:io/streams#[resource-drop]input-stream" dropInputStreamRaw :: Resource InputStream -> Unit
foreign import "wasi:io/streams#[resource-drop]output-stream" dropOutputStreamRaw :: Resource OutputStream -> Unit
foreign import "wasi:io/poll#[resource-drop]pollable" dropPollableRaw :: Resource Pollable -> Unit
foreign import "wasi:io/error#[resource-drop]error" dropErrorRaw :: Resource Error -> Unit

getStdin :: Effect (Resource InputStream)
getStdin = \token -> getStdinRaw

getStdout :: Effect (Resource OutputStream)
getStdout = \token -> getStdoutRaw

getStderr :: Effect (Resource OutputStream)
getStderr = \token -> getStderrRaw

read :: Resource InputStream -> Int -> Effect (Either StreamError String)
read stream len = \token -> readRaw stream len

blockingRead :: Resource InputStream -> Int -> Effect (Either StreamError String)
blockingRead stream len = \token -> blockingReadRaw stream len

skip :: Resource InputStream -> Int -> Effect (Either StreamError Int)
skip stream len = \token -> skipRaw stream len

blockingSkip :: Resource InputStream -> Int -> Effect (Either StreamError Int)
blockingSkip stream len = \token -> blockingSkipRaw stream len

write :: Resource OutputStream -> String -> Effect (Either StreamError Unit)
write stream contents = \token -> writeRaw stream contents

blockingWriteAndFlush :: Resource OutputStream -> String -> Effect (Either StreamError Unit)
blockingWriteAndFlush stream contents = \token ->
  blockingWriteAndFlushRaw stream contents

flush :: Resource OutputStream -> Effect (Either StreamError Unit)
flush stream = \token -> flushRaw stream

subscribeInputStream :: Resource InputStream -> Effect (Resource Pollable)
subscribeInputStream stream = \token -> subscribeInputStreamRaw stream

subscribeOutputStream :: Resource OutputStream -> Effect (Resource Pollable)
subscribeOutputStream stream = \token -> subscribeOutputStreamRaw stream

toDebugString :: Resource Error -> Effect String
toDebugString err = \token -> toDebugStringRaw err

poll :: Array (Resource Pollable) -> Effect (Array Int)
poll pollables = \token -> pollRaw pollables

ready :: Resource Pollable -> Effect Boolean
ready pollable = \token -> readyRaw pollable

block :: Resource Pollable -> Effect Unit
block pollable = \token -> blockRaw pollable

dropInputStream :: Resource InputStream -> Effect Unit
dropInputStream stream = \token -> dropInputStreamRaw stream

dropOutputStream :: Resource OutputStream -> Effect Unit
dropOutputStream stream = \token -> dropOutputStreamRaw stream

dropPollable :: Resource Pollable -> Effect Unit
dropPollable pollable = \token -> dropPollableRaw pollable

dropError :: Resource Error -> Effect Unit
dropError err = \token -> dropErrorRaw err

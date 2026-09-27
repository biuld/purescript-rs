module WASI.Streams
  ( InputStream
  , OutputStream
  , Pollable
  , Error
  , StreamError(..)
  , getStdout
  , getStderr
  , read
  , blockingRead
  , skip
  , blockingSkip
  , blockingWriteAndFlush
  , subscribeInputStream
  , subscribeOutputStream
  , dropInputStream
  , dropOutputStream
  , dropPollable
  , dropError
  ) where

import Prelude
import Data.Either (Either)

foreign import data InputStream :: Type
foreign import data OutputStream :: Type
foreign import data Pollable :: Type
foreign import data Error :: Type

data StreamError = LastOperationFailed Error | Closed

foreign import "wasi:cli/stdout#get-stdout" getStdoutRaw :: OutputStream
foreign import "wasi:cli/stderr#get-stderr" getStderrRaw :: OutputStream
foreign import "wasi:io/streams#[method]input-stream.read" readRaw :: InputStream -> Int -> Either String StreamError
foreign import "wasi:io/streams#[method]input-stream.blocking-read" blockingReadRaw :: InputStream -> Int -> Either String StreamError
foreign import "wasi:io/streams#[method]input-stream.skip" skipRaw :: InputStream -> Int -> Either Int StreamError
foreign import "wasi:io/streams#[method]input-stream.blocking-skip" blockingSkipRaw :: InputStream -> Int -> Either Int StreamError
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush" blockingWriteAndFlushRaw :: OutputStream -> String -> Either Unit StreamError
foreign import "wasi:io/streams#[method]input-stream.subscribe" subscribeInputStreamRaw :: InputStream -> Pollable
foreign import "wasi:io/streams#[method]output-stream.subscribe" subscribeOutputStreamRaw :: OutputStream -> Pollable
foreign import "wasi:io/streams#[resource-drop]input-stream" dropInputStreamRaw :: InputStream -> Unit
foreign import "wasi:io/streams#[resource-drop]output-stream" dropOutputStreamRaw :: OutputStream -> Unit
foreign import "wasi:io/poll#[resource-drop]pollable" dropPollableRaw :: Pollable -> Unit
foreign import "wasi:io/error#[resource-drop]error" dropErrorRaw :: Error -> Unit

getStdout :: Effect OutputStream
getStdout = \token -> getStdoutRaw

getStderr :: Effect OutputStream
getStderr = \token -> getStderrRaw

read :: InputStream -> Int -> Effect (Either String StreamError)
read stream len = \token -> readRaw stream len

blockingRead :: InputStream -> Int -> Effect (Either String StreamError)
blockingRead stream len = \token -> blockingReadRaw stream len

skip :: InputStream -> Int -> Effect (Either Int StreamError)
skip stream len = \token -> skipRaw stream len

blockingSkip :: InputStream -> Int -> Effect (Either Int StreamError)
blockingSkip stream len = \token -> blockingSkipRaw stream len

blockingWriteAndFlush :: OutputStream -> String -> Effect (Either Unit StreamError)
blockingWriteAndFlush stream contents = \token ->
  blockingWriteAndFlushRaw stream contents

subscribeInputStream :: InputStream -> Effect Pollable
subscribeInputStream stream = \token -> subscribeInputStreamRaw stream

subscribeOutputStream :: OutputStream -> Effect Pollable
subscribeOutputStream stream = \token -> subscribeOutputStreamRaw stream

dropInputStream :: InputStream -> Effect Unit
dropInputStream stream = \token -> dropInputStreamRaw stream

dropOutputStream :: OutputStream -> Effect Unit
dropOutputStream stream = \token -> dropOutputStreamRaw stream

dropPollable :: Pollable -> Effect Unit
dropPollable pollable = \token -> dropPollableRaw pollable

dropError :: Error -> Effect Unit
dropError err = \token -> dropErrorRaw err

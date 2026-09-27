module WASI.Console (log, error) where

import Prelude
import WASI.Resource (Resource(..), unResource, withResource)

foreign import "wasi:cli/stdout#get-stdout" getStdout :: Int
foreign import "wasi:cli/stderr#get-stderr" getStderr :: Int
foreign import "wasi:io/streams#[resource-drop]output-stream" dropStreamRaw :: Int -> Unit
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush" writeStream :: Int -> String -> Unit

dropStream :: Resource Int -> Effect Unit
dropStream resource = \token -> dropStreamRaw (unResource resource)

log :: String -> Effect Unit
log s = \token ->
  withResource dropStream (Resource getStdout) (\resource -> \t ->
    let ignored = writeStream (unResource resource) s in
    writeStream (unResource resource) "\n") token

error :: String -> Effect Unit
error s = \token ->
  withResource dropStream (Resource getStderr) (\resource -> \t ->
    let ignored = writeStream (unResource resource) s in
    writeStream (unResource resource) "\n") token

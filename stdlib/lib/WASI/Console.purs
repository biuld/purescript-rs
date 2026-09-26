
module WASI.Console (log, error) where

import Prelude

foreign import "wasi:cli/stdout#get-stdout" getStdout :: Int
foreign import "wasi:cli/stderr#get-stderr" getStderr :: Int
foreign import "wasi:io/streams#[method]output-stream.blocking-write-and-flush" writeStdout :: Int -> String -> Unit

log :: String -> Effect Unit
log s = \token -> let ignored = writeStdout getStdout s in writeStdout getStdout "\n"

error :: String -> Effect Unit
error s = \token -> let ignored = writeStdout getStderr s in writeStdout getStderr "\n"

module WASI.Random
  ( randomBytes
  , randomU64
  , insecureBytes
  , insecureU64
  , insecureSeed
  ) where

import Prelude

foreign import "wasi:random/random#get-random-bytes" randomBytes :: Int -> Effect (String)
foreign import "wasi:random/random#get-random-u64" randomU64 :: Effect (Int)
foreign import "wasi:random/insecure#get-insecure-random-bytes" insecureBytes :: Int -> Effect (String)
foreign import "wasi:random/insecure#get-insecure-random-u64" insecureU64 :: Effect (Int)
foreign import "wasi:random/insecure-seed#insecure-seed" insecureSeed :: Effect ({ _1 :: Int, _2 :: Int })


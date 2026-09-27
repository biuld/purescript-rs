module WASI.Random
  ( randomBytes
  , randomU64
  , insecureBytes
  , insecureU64
  , insecureSeed
  ) where

import Prelude

foreign import "wasi:random/random#get-random-bytes" getRandomBytes :: Int -> String
foreign import "wasi:random/random#get-random-u64" getRandomU64 :: Int
foreign import "wasi:random/insecure#get-insecure-random-bytes" getInsecureRandomBytes :: Int -> String
foreign import "wasi:random/insecure#get-insecure-random-u64" getInsecureRandomU64 :: Int
foreign import "wasi:random/insecure-seed#insecure-seed" insecureSeedRaw :: { _1 :: Int, _2 :: Int }

randomBytes :: Int -> Effect String
randomBytes count = \token -> getRandomBytes count

randomU64 :: Effect Int
randomU64 = \token -> getRandomU64

insecureBytes :: Int -> Effect String
insecureBytes count = \token -> getInsecureRandomBytes count

insecureU64 :: Effect Int
insecureU64 = \token -> getInsecureRandomU64

insecureSeed :: Effect { _1 :: Int, _2 :: Int }
insecureSeed = \token -> insecureSeedRaw

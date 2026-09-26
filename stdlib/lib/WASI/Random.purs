module WASI.Random (randomBytes, randomU64) where

import Prelude

foreign import "wasi:random/random#get-random-bytes" getRandomBytes :: Int -> String

foreign import "wasi:random/random#get-random-u64" getRandomU64 :: Int

randomBytes :: Int -> Effect String
randomBytes count = \token -> getRandomBytes count

randomU64 :: Effect Int
randomU64 = \token -> getRandomU64

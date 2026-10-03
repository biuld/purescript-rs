module Main where

import Prim.TypeError (class Warn, Text)

warned :: forall a. Warn (Text "local let") => a -> a
warned value = value

outer :: Int
outer = let inner = warned 1 in inner

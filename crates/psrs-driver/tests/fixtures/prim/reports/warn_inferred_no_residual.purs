module Main where

import Prim.TypeError (class Warn, Quote)

warned :: forall a. Warn (Quote a) => a -> a
warned value = value

inferred value = warned value

concrete :: Int
concrete = inferred 5

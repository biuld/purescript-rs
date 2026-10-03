module Main where

import Prim.TypeError (class Fail, Text)

class Thing a where
  thing :: a

instance failThing :: Fail (Text "custom residual") => Thing Int where
  thing = 1

bar :: Int
bar = thing

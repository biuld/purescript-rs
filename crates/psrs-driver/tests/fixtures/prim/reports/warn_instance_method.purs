module Main where

import Prim.TypeError (class Warn, Text)

warned :: forall a. Warn (Text "instance method") => a -> a
warned value = value

class Run a where
  run :: a -> a

instance runInt :: Run Int where
  run value = warned value

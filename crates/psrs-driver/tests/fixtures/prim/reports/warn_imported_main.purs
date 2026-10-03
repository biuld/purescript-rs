module Main where
import Prim.TypeError (class Warn, Text)
import Lib (foo)

given :: Warn (Text "from library") => Int
given = foo 1

trigger :: Int
trigger = foo 2

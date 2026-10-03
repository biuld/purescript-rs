module Lib where
import Prim.TypeError (class Warn, Text)

foo :: Warn (Text "from library") => Int -> Int
foo value = value

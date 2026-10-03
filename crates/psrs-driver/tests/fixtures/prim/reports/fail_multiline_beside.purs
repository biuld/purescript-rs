module Main where

import Prim.TypeError (class Fail, Text, Above, Beside)

foo :: Fail (Beside (Above (Text "long") (Text "x")) (Text "!")) => Int
foo = 1

bar :: Int
bar = foo

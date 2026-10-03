module Main where

import Prim.TypeError (class Fail, Above, Beside, QuoteLabel, Text)

foo :: Int
foo = (0 :: Fail (Above (Beside (Text "bad ") (QuoteLabel "h e l l o")) (Text "value")) => Int)

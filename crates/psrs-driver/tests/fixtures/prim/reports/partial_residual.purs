module Main where

import Prim (class Partial)

data Box = Box

foo :: Partial => Box
foo = Box

bar :: Box
bar = foo

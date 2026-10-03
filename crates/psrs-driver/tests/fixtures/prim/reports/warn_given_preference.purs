module Main where

import Prim.TypeError

foo :: Warn (Text "foo") => Int -> Int
foo x = x

-- The first warning is given and must not fire here. Both constraints are
-- propagated to baz, where neither warning is in scope.
bar :: Warn (Text "foo") => Warn (Text "bar") => Int
bar = foo 42

baz :: Int
baz = bar

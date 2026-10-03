module Main where

class Identity a where
  identity :: a -> a

instance identityInt :: Identity Int where
  identity value = value

poly :: forall a. Identity a => a -> a
poly value = identity value

applyPoly :: (forall a. Identity a => a -> a) -> Int
applyPoly function = function 42

main :: Int
main = applyPoly (poly :: forall a. Identity a => a -> a)

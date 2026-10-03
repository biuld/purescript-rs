module Main where
import Prim.TypeError (class Fail, Text)

identity :: forall a. Fail (Text "polymorphic failure") => a -> a
identity value = value

mono :: Int -> Int
mono = (identity :: forall a. Fail (Text "polymorphic failure") => a -> a)

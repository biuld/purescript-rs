module Main where
import Prim.TypeError (class Warn, Text)

identity :: forall a. Warn (Text "polymorphic warning") => a -> a
identity value = value

mono :: Int -> Int
mono = (identity :: forall a. Warn (Text "polymorphic warning") => a -> a)

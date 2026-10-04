-- | The `Functor` class and the `<$>` operator.
-- |
-- | `map` is the class method. `Prelude` re-exports it and supplies the
-- | `Effect` instance, because this module cannot import `Prelude` without a
-- | cycle. The `Array` instance walks indexes with `arrayIndex` and builds the
-- | result with `arrayAppend`, the same primitives the rest of the library
-- | uses for arrays.
module Data.Functor
  ( class Functor
  , map
  , (<$>)
  ) where

import Data.Either (Either(..))
import Data.Maybe (Maybe(..))

-- | A type constructor that can apply a function to its contents.
class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b

infixl 4 map as <$>

instance functorArray :: Functor Array where
  map f xs = mapFrom f xs 0

instance functorMaybe :: Functor Maybe where
  map _ Nothing = Nothing
  map f (Just value) = Just (f value)

instance functorEither :: Functor (Either a) where
  map _ (Left value) = Left value
  map f (Right value) = Right (f value)

-- | `mapFrom f xs i` is `f xs[i]` followed by the rest. The recursive call is
-- | an argument of `arrayAppend`, so this copies the tail at each index.
mapFrom :: forall a b. (a -> b) -> Array a -> Int -> Array b
mapFrom f xs index =
  if intLt index (arrayLength xs) then
    arrayAppend [f (arrayIndex xs index)] (mapFrom f xs (intAdd index 1))
  else
    []

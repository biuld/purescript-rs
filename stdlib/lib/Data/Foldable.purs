-- | The `Foldable` class.
-- |
-- | This is the class surface deriving and the fundep work assume: `foldr`,
-- | `foldl`, and `foldMap`, not a second traversal implementation. The `Array`
-- | instance walks indexes with the compiler's `arrayLength` and `arrayIndex`
-- | primitives. `Maybe` and `Either` fold by cases. `fold` is omitted. It would be `foldMap` of the identity under a
-- | quantified -- | `Foldable f`, and projecting that rank-2 method from a dictionary parameter
-- | fails CC verification, which rejects every program that does not prune
-- | unreachable library declarations. Combinators that need
-- | `Applicative`, `Alt`, or `Ord` stay out until those classes exist.
module Data.Foldable
  ( class Foldable
  , foldr
  , foldl
  , foldMap
  ) where

import Data.Either (Either(..))
import Data.Maybe (Maybe(..))
import Data.Monoid (class Monoid, mempty)
import Data.Semigroup (class Semigroup, append)

-- | A container that can be folded.
class Foldable f where
  foldr :: forall a b. (a -> b -> b) -> b -> f a -> b
  foldl :: forall a b. (b -> a -> b) -> b -> f a -> b
  foldMap :: forall a m. Monoid m => (a -> m) -> f a -> m

instance foldableArray :: Foldable Array where
  foldr f z xs = foldrIndex f z xs 0
  foldl f z xs = foldlIndex f z xs 0
  foldMap f xs = foldr (\x acc -> append (f x) acc) mempty xs

instance foldableMaybe :: Foldable Maybe where
  foldr _ z Nothing = z
  foldr f z (Just x) = f x z
  foldl _ z Nothing = z
  foldl f z (Just x) = f z x
  foldMap _ Nothing = mempty
  foldMap f (Just x) = f x

instance foldableEither :: Foldable (Either a) where
  foldr _ z (Left _) = z
  foldr f z (Right x) = f x z
  foldl _ z (Left _) = z
  foldl f z (Right x) = f z x
  foldMap _ (Left _) = mempty
  foldMap f (Right x) = f x

-- | Left-to-right index, right-to-left combination: `go i` is
-- | `f xs[i] (go (i + 1))`, which is `foldr`. `intLt` is the internal
-- | comparison, so this module does not import the `Ord` operator.
foldrIndex :: forall a b. (a -> b -> b) -> b -> Array a -> Int -> b
foldrIndex f z xs i =
  if intLt i (arrayLength xs) then
    f (arrayIndex xs i) (foldrIndex f z xs (intAdd i 1))
  else
    z

-- | Left-to-right walk. The recursive call is in tail position.
foldlIndex :: forall a b. (b -> a -> b) -> b -> Array a -> Int -> b
foldlIndex f z xs i =
  if intLt i (arrayLength xs) then
    foldlIndex f (f z (arrayIndex xs i)) xs (intAdd i 1)
  else
    z

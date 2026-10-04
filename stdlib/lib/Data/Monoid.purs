-- | The `Monoid` class.
-- |
-- | `Monoid` is the superclass constraint on `Data.Foldable.foldMap` and
-- | `fold`. The class and its `mempty` method are the official surface; the
-- | instances are the three types whose `Semigroup` instances already exist,
-- | so folding those types does not invent a second appending operation.
module Data.Monoid
  ( class Monoid
  , mempty
  ) where

import Data.Semigroup (class Semigroup)

-- | A `Semigroup` with an identity element.
class Semigroup m <= Monoid m where
  mempty :: m

instance monoidString :: Monoid String where
  mempty = ""

instance monoidUnit :: Monoid Unit where
  mempty = unit

instance monoidArray :: Monoid (Array a) where
  mempty = []

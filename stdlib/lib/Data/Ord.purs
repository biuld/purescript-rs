-- | The `Ord` class and its `<`, `<=`, `>`, and `>=` operators.
-- |
-- | `Ord` is a subclass of `Eq`. Each instance is defined by the four
-- | comparison intrinsics for the ordered types; `Boolean` is ordered by
-- | `false < true` and needs no intrinsic.
module Data.Ord
  ( class Ord
  , lessThan
  , lessThanOrEq
  , greaterThan
  , greaterThanOrEq
  , (<)
  , (<=)
  , (>)
  , (>=)
  ) where

import Data.Eq (class Eq)

-- | A type with a total order. Its `Eq` instance agrees with the order.
class Eq a <= Ord a where
  lessThan :: a -> a -> Boolean
  lessThanOrEq :: a -> a -> Boolean
  greaterThan :: a -> a -> Boolean
  greaterThanOrEq :: a -> a -> Boolean

infix 4 lessThan as <
infix 4 lessThanOrEq as <=
infix 4 greaterThan as >
infix 4 greaterThanOrEq as >=

instance ordInt :: Ord Int where
  lessThan x y = intLt x y
  lessThanOrEq x y = intLe x y
  greaterThan x y = intGt x y
  greaterThanOrEq x y = intGe x y

instance ordNumber :: Ord Number where
  lessThan x y = numberLt x y
  lessThanOrEq x y = numberLe x y
  greaterThan x y = numberGt x y
  greaterThanOrEq x y = numberGe x y

instance ordChar :: Ord Char where
  lessThan x y = charLt x y
  lessThanOrEq x y = charLe x y
  greaterThan x y = charGt x y
  greaterThanOrEq x y = charGe x y

instance ordBoolean :: Ord Boolean where
  lessThan left right = if left then false else right
  lessThanOrEq left right = if left then right else true
  greaterThan left right = if right then false else left
  greaterThanOrEq left right = if right then left else true

-- | The `Semiring` class and its `+` / `*` operators.
-- |
-- | The `Int` and `Number` instances are the compiler's internal arithmetic
-- | intrinsics; the surface operators are library declarations over them.
module Data.Semiring
  ( class Semiring
  , add
  , mul
  , (+)
  , (*)
  ) where

-- | A type with addition and multiplication.
class Semiring a where
  add :: a -> a -> a
  mul :: a -> a -> a

infixl 6 add as +
infixl 7 mul as *

instance semiringInt :: Semiring Int where
  add x y = intAdd x y
  mul x y = intMul x y

instance semiringNumber :: Semiring Number where
  add x y = numberAdd x y
  mul x y = numberMul x y

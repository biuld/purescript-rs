-- | The `Eq` class and its `==` / `/=` operators.
-- |
-- | The `Int`, `Number`, `Boolean`, and `Char` instances are the compiler's
-- | internal equality intrinsics; `Unit` has one value, so it is equal to
-- | itself. The surface operators are library declarations over the internal
-- | primitives, so the primitive names stay out of the source namespace.
module Data.Eq
  ( class Eq
  , eq
  , notEq
  , (==)
  , (/=)
  ) where

-- | A type whose values can be compared for equality.
class Eq a where
  eq :: a -> a -> Boolean
  notEq :: a -> a -> Boolean

infix 4 eq as ==
infix 4 notEq as /=

instance eqInt :: Eq Int where
  eq x y = intEq x y
  notEq x y = intNe x y

instance eqNumber :: Eq Number where
  eq x y = numberEq x y
  notEq x y = numberNe x y

instance eqBoolean :: Eq Boolean where
  eq x y = booleanEq x y
  notEq x y = booleanNe x y

instance eqChar :: Eq Char where
  eq x y = charEq x y
  notEq x y = charNe x y

instance eqUnit :: Eq Unit where
  eq _ _ = true
  notEq _ _ = false

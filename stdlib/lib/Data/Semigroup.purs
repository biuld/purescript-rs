-- | The `Semigroup` class and its `<>` operator.
-- |
-- | This is the first class the library itself declares. The class, its
-- | method, and the operator alias are all re-exported from `Prelude`, which
-- | is what the corpus imports.
-- |
-- | `append` for `String` is built from the effect-free `stringToBytes` /
-- | `arrayAppend` / `bytesToString` primitives rather than a second string
-- | concatenation path. Joining two well-formed UTF-8 byte sequences always
-- | yields well-formed UTF-8, so `bytesToString`'s validation is a no-op here
-- | and the one string representation stays the only writer
-- | ([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
module Data.Semigroup
  ( class Semigroup
  , append
  , (<>)
  ) where

-- | A type with an associative binary operation.
class Semigroup a where
  append :: a -> a -> a

infixr 5 append as <>

instance semigroupString :: Semigroup String where
  append left right = bytesToString (arrayAppend (stringToBytes left) (stringToBytes right))

instance semigroupUnit :: Semigroup Unit where
  append _ _ = unit

instance semigroupArray :: Semigroup (Array a) where
  append left right = arrayAppend left right

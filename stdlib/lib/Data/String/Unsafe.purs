-- | Unsafe string and character functions.
module Data.String.Unsafe
  ( char
  , charAt
  ) where

-- | Returns the character at the given index.
-- |
-- | **Unsafe:** throws runtime exception if the index is out of bounds.
charAt :: Int -> String -> Char
charAt a0 a1 = charAt a0 a1

-- | Converts a string of length `1` to a character.
-- |
-- | **Unsafe:** throws runtime exception if length is not `1`.
char :: String -> Char
char a0 = char a0

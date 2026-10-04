-- | This module defines bitwise operations for the `Int` type.
module Data.Int.Bits
  ( and, (.&.)
  , or, (.|.)
  , xor, (.^.)
  , shl
  , shr
  , zshr
  , complement
  ) where

-- | Bitwise AND.
and :: Int -> Int -> Int
and a0 a1 = intAnd a0 a1

infixl 10 and as .&.

-- | Bitwise OR.
or :: Int -> Int -> Int
or a0 a1 = intOr a0 a1

infixl 10 or as .|.

-- | Bitwise XOR.
xor :: Int -> Int -> Int
xor a0 a1 = intXor a0 a1

infixl 10 xor as .^.

-- | Bitwise shift left.
shl :: Int -> Int -> Int
shl a0 a1 = intShl a0 a1

-- | Bitwise shift right.
shr :: Int -> Int -> Int
shr a0 a1 = intShr a0 a1

-- | Bitwise zero-fill shift right.
zshr :: Int -> Int -> Int
zshr a0 a1 = intZshr a0 a1

-- | Bitwise NOT.
complement :: Int -> Int
complement a0 = intComplement a0

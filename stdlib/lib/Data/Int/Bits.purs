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
foreign import "psrs:intrinsic#intAnd" and :: Int -> Int -> Int

infixl 10 and as .&.

-- | Bitwise OR.
foreign import "psrs:intrinsic#intOr" or :: Int -> Int -> Int

infixl 10 or as .|.

-- | Bitwise XOR.
foreign import "psrs:intrinsic#intXor" xor :: Int -> Int -> Int

infixl 10 xor as .^.

-- | Bitwise shift left.
foreign import "psrs:intrinsic#intShl" shl :: Int -> Int -> Int

-- | Bitwise shift right.
foreign import "psrs:intrinsic#intShr" shr :: Int -> Int -> Int

-- | Bitwise zero-fill shift right.
foreign import "psrs:intrinsic#intZshr" zshr :: Int -> Int -> Int

-- | Bitwise NOT.
foreign import "psrs:intrinsic#intComplement" complement :: Int -> Int

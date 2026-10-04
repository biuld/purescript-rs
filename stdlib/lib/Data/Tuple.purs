-- | A pair, as the closed record `{ _1, _2 }`.
-- |
-- | FE-06 already lowers tuple syntax to that record, and DEC-13 maps a WIT
-- | `tuple<A, B>` to the same labels. This module is that record, not a second
-- | product: `Tuple a b` and `{ _1 :: a, _2 :: b }` are one type, and `(x, y)`
-- | is a value of it. There is no algebraic `Tuple` constructor and no
-- | `Eq` / `Ord` / `Show` / `Functor` instance; those would either duplicate
-- | the record or belong to the Prelude class slices.
module Data.Tuple
  ( Tuple
  , fst
  , snd
  , curry
  , uncurry
  , swap
  ) where

type Tuple a b = { _1 :: a, _2 :: b }

-- | The first component. `fst (x, y)` is `x`.
fst :: forall a b. Tuple a b -> a
fst tuple = tuple._1

-- | The second component. `snd (x, y)` is `y`.
snd :: forall a b. Tuple a b -> b
snd tuple = tuple._2

-- | Turns a function of a pair into a function of two arguments.
curry :: forall a b c. (Tuple a b -> c) -> a -> b -> c
curry f x y = f { _1: x, _2: y }

-- | Turns a function of two arguments into a function of a pair.
uncurry :: forall a b c. (a -> b -> c) -> Tuple a b -> c
uncurry f tuple = f (tuple._1) (tuple._2)

-- | Exchanges the two components.
swap :: forall a b. Tuple a b -> Tuple b a
swap tuple = { _1: tuple._2, _2: tuple._1 }

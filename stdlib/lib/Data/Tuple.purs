-- | A strict product of two values. Native tuple syntax remains a closed
-- | record, while this library type provides the constructor used by the core
-- | libraries. WIT tuples continue to map to closed records as specified by
-- | DEC-13.
module Data.Tuple
  ( Tuple(..)
  , fst
  , snd
  , curry
  , uncurry
  , swap
  ) where

import Data.Eq (class Eq)
import Data.Functor (class Functor)
import Data.Ord (class Ord)
import Data.Show (class Show, show)
import Data.Semigroup ((<>))

data Tuple a b = Tuple a b

derive instance eqTuple :: (Eq a, Eq b) => Eq (Tuple a b)
derive instance ordTuple :: (Ord a, Ord b) => Ord (Tuple a b)
derive instance functorTuple :: Functor (Tuple a)

instance showTuple :: (Show a, Show b) => Show (Tuple a b) where
  show (Tuple first second) = "(Tuple " <> show first <> " " <> show second <> ")"

-- | The first component. `fst (Tuple x y)` is `x`.
fst :: forall a b. Tuple a b -> a
fst (Tuple first _) = first

-- | The second component. `snd (Tuple x y)` is `y`.
snd :: forall a b. Tuple a b -> b
snd (Tuple _ second) = second

-- | Turns a function of a pair into a function of two arguments.
curry :: forall a b c. (Tuple a b -> c) -> a -> b -> c
curry f x y = f (Tuple x y)

-- | Turns a function of two arguments into a function of a pair.
uncurry :: forall a b c. (a -> b -> c) -> Tuple a b -> c
uncurry f (Tuple first second) = f first second

-- | Exchanges the two components.
swap :: forall a b. Tuple a b -> Tuple b a
swap (Tuple first second) = Tuple second first

-- | Helper functions for working with mutable arrays using the `ST` effect.
-- |
-- | This module can be used when performance is important and mutation is a local effect.

module Data.Array.ST
  ( STArray(..)
  , Assoc
  , run
  , withArray
  , new
  , peek
  , poke
  , modify
  , length
  , pop
  , push
  , pushAll
  , shift
  , unshift
  , unshiftAll
  , splice
  , sort
  , sortBy
  , sortWith
  , freeze
  , thaw
  , clone
  , unsafeFreeze
  , unsafeThaw
  , toAssocArray
  ) where

import Prelude

import Control.Monad.ST (ST, Region)
import Control.Monad.ST as ST
import Control.Monad.ST.Uncurried (STFn1, STFn2, STFn3, STFn4, runSTFn1, runSTFn2, runSTFn3, runSTFn4)
import Data.Maybe (Maybe(..))

-- | A reference to a mutable array.
-- |
-- | The first type parameter represents the memory region which the array belongs to.
-- | The second type parameter defines the type of elements of the mutable array.
-- |
-- | The runtime representation of a value of type `STArray h a` is the same as that of `Array a`,
-- | except that mutation is allowed.
foreign import data STArray :: Region -> Type -> Type

type role STArray nominal representational

-- | An element and its index.
type Assoc a = { value :: a, index :: Int }

-- | A safe way to create and work with a mutable array before returning an
-- | immutable array for later perusal. This function avoids copying the array
-- | before returning it - it uses unsafeFreeze internally, but this wrapper is
-- | a safe interface to that function.
run :: forall a. (forall h. ST h (STArray h a)) -> Array a
run st = ST.run (st >>= unsafeFreeze)

-- | Perform an effect requiring a mutable array on a copy of an immutable array,
-- | safely returning the result as an immutable array.
withArray
  :: forall h a b
   . (STArray h a -> ST h b)
  -> Array a
  -> ST h (Array a)
withArray f xs = do
  result <- thaw xs
  _ <- f result
  unsafeFreeze result

-- | O(1). Convert a mutable array to an immutable array, without copying. The mutable
-- | array must not be mutated afterwards.
unsafeFreeze :: forall h a. STArray h a -> ST h (Array a)
unsafeFreeze = runSTFn1 unsafeFreezeImpl

unsafeFreezeImpl :: forall h a. STFn1 (STArray h a) h (Array a)
unsafeFreezeImpl = unsafeFreezeImpl

-- | O(1) Convert an immutable array to a mutable array, without copying. The input
-- | array must not be used afterward.
unsafeThaw :: forall h a. Array a -> ST h (STArray h a)
unsafeThaw = runSTFn1 unsafeThawImpl

unsafeThawImpl :: forall h a. STFn1 (Array a) h (STArray h a)
unsafeThawImpl = unsafeThawImpl

-- | Create a new, empty mutable array.
new :: forall h a. ST h (STArray h a)
new = new

thaw
  :: forall h a
   . Array a
  -> ST h (STArray h a)
thaw = runSTFn1 thawImpl

-- | Create a mutable copy of an immutable array.
thawImpl :: forall h a. STFn1 (Array a) h (STArray h a)
thawImpl = thawImpl

-- | Make a mutable copy of a mutable array.
clone
  :: forall h a
   . STArray h a
  -> ST h (STArray h a)
clone = runSTFn1 cloneImpl

cloneImpl :: forall h a. STFn1 (STArray h a) h (STArray h a)
cloneImpl = cloneImpl

-- | Sort a mutable array in place. Sorting is stable: the order of equal
-- | elements is preserved.
sort :: forall a h. Ord a => STArray h a -> ST h (STArray h a)
sort = sortBy compare

-- | Remove the first element from an array and return that element.
shift :: forall h a. STArray h a -> ST h (Maybe a)
shift = runSTFn3 shiftImpl Just Nothing

shiftImpl :: forall h a . STFn3 (forall b. b -> Maybe b) (forall b. Maybe b) (STArray h a) h (Maybe a)
shiftImpl = shiftImpl

-- | Sort a mutable array in place using a comparison function. Sorting is
-- | stable: the order of elements is preserved if they are equal according to
-- | the comparison function.
sortBy
  :: forall a h
   . (a -> a -> Ordering)
  -> STArray h a
  -> ST h (STArray h a)
sortBy comp = runSTFn3 sortByImpl comp case _ of
  GT -> 1
  EQ -> 0
  LT -> -1

sortByImpl :: forall a h . STFn3 (a -> a -> Ordering) (Ordering -> Int) (STArray h a) h (STArray h a)
sortByImpl = sortByImpl

-- | Sort a mutable array in place based on a projection. Sorting is stable: the
-- | order of elements is preserved if they are equal according to the projection.
sortWith
  :: forall a b h
   . Ord b
  => (a -> b)
  -> STArray h a
  -> ST h (STArray h a)
sortWith f = sortBy (comparing f)

-- | Create an immutable copy of a mutable array.
freeze
  :: forall h a
   . STArray h a
  -> ST h (Array a)
freeze = runSTFn1 freezeImpl

freezeImpl :: forall h a. STFn1 (STArray h a) h (Array a)
freezeImpl = freezeImpl

-- | Read the value at the specified index in a mutable array.
peek
  :: forall h a
   . Int
  -> STArray h a
  -> ST h (Maybe a)
peek = runSTFn4 peekImpl Just Nothing

peekImpl :: forall h a r. STFn4 (a -> r) r Int (STArray h a) h r
peekImpl = peekImpl

poke
  :: forall h a
   . Int
  -> a
  -> STArray h a
  -> ST h Boolean
poke = runSTFn3 pokeImpl

-- | Change the value at the specified index in a mutable array.
pokeImpl :: forall h a. STFn3 Int a (STArray h a) h Boolean
pokeImpl = pokeImpl

lengthImpl :: forall h a. STFn1 (STArray h a) h Int
lengthImpl = lengthImpl

-- | Get the number of elements in a mutable array.
length :: forall h a. STArray h a -> ST h Int
length = runSTFn1 lengthImpl

-- | Remove the last element from an array and return that element.
pop :: forall h a. STArray h a -> ST h (Maybe a)
pop = runSTFn3 popImpl Just Nothing

popImpl :: forall h a . STFn3 (forall b. b -> Maybe b) (forall b. Maybe b) (STArray h a) h (Maybe a)
popImpl = popImpl

-- | Append an element to the end of a mutable array. Returns the new length of
-- | the array.
push :: forall h a. a -> (STArray h a) -> ST h Int
push = runSTFn2 pushImpl

pushImpl :: forall h a. STFn2 a (STArray h a) h Int
pushImpl = pushImpl

-- | Append the values in an immutable array to the end of a mutable array.
-- | Returns the new length of the mutable array.
pushAll
  :: forall h a
   . Array a
  -> STArray h a
  -> ST h Int
pushAll = runSTFn2 pushAllImpl

pushAllImpl :: forall h a . STFn2 (Array a) (STArray h a) h Int
pushAllImpl = pushAllImpl

-- | Append an element to the front of a mutable array. Returns the new length of
-- | the array.
unshift :: forall h a. a -> STArray h a -> ST h Int
unshift a = runSTFn2 unshiftAllImpl [ a ]

-- | Append the values in an immutable array to the front of a mutable array.
-- | Returns the new length of the mutable array.
unshiftAll
  :: forall h a
   . Array a
  -> STArray h a
  -> ST h Int
unshiftAll = runSTFn2 unshiftAllImpl

unshiftAllImpl :: forall h a . STFn2 (Array a) (STArray h a) h Int
unshiftAllImpl = unshiftAllImpl

-- | Mutate the element at the specified index using the supplied function.
modify :: forall h a. Int -> (a -> a) -> STArray h a -> ST h Boolean
modify i f xs = do
  entry <- peek i xs
  case entry of
    Just x -> poke i (f x) xs
    Nothing -> pure false

-- | Remove and/or insert elements from/into a mutable array at the specified index.
splice
  :: forall h a
   . Int
  -> Int
  -> Array a
  -> STArray h a
  -> ST h (Array a)
splice = runSTFn4 spliceImpl

spliceImpl :: forall h a . STFn4 Int Int (Array a) (STArray h a) h (Array a)
spliceImpl = spliceImpl

-- | Create an immutable copy of a mutable array, where each element
-- | is labelled with its index in the original array.
toAssocArray
  :: forall h a
   . STArray h a
  -> ST h (Array (Assoc a))
toAssocArray = runSTFn1 toAssocArrayImpl

toAssocArrayImpl :: forall h a . STFn1 (STArray h a) h (Array (Assoc a))
toAssocArrayImpl = toAssocArrayImpl

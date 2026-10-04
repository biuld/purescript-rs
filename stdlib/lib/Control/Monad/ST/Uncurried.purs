-- | This module defines types for STf uncurried functions, as well as
-- | functions for converting back and forth between them.
-- |
-- | The general naming scheme for functions and types in this module is as
-- | follows:
-- |
-- | * `STFn{N}` means, an uncurried function which accepts N arguments and
-- |   performs some STs. The first N arguments are the actual function's
-- |   argument. The last type argument is the return type.
-- | * `runSTFn{N}` takes an `STFn` of N arguments, and converts it into
-- |   the normal PureScript form: a curried function which returns an ST
-- |   action.
-- | * `mkSTFn{N}` is the inverse of `runSTFn{N}`. It can be useful for
-- |   callbacks.
-- |

module Control.Monad.ST.Uncurried where

import Control.Monad.ST.Internal (ST, Region)

foreign import data STFn1 :: Type -> Region -> Type -> Type

type role STFn1 representational nominal representational

foreign import data STFn2 :: Type -> Type -> Region -> Type -> Type

type role STFn2 representational representational nominal representational

foreign import data STFn3 :: Type -> Type -> Type -> Region -> Type -> Type

type role STFn3 representational representational representational nominal representational

foreign import data STFn4 :: Type -> Type -> Type -> Type -> Region -> Type -> Type

type role STFn4 representational representational representational representational nominal representational

foreign import data STFn5 :: Type -> Type -> Type -> Type -> Type -> Region -> Type -> Type

type role STFn5 representational representational representational representational representational nominal representational

foreign import data STFn6 :: Type -> Type -> Type -> Type -> Type -> Type -> Region -> Type -> Type

type role STFn6 representational representational representational representational representational representational nominal representational

foreign import data STFn7 :: Type -> Type -> Type -> Type -> Type -> Type -> Type -> Region -> Type -> Type

type role STFn7 representational representational representational representational representational representational representational nominal representational

foreign import data STFn8 :: Type -> Type -> Type -> Type -> Type -> Type -> Type -> Type -> Region -> Type -> Type

type role STFn8 representational representational representational representational representational representational representational representational nominal representational

foreign import data STFn9 :: Type -> Type -> Type -> Type -> Type -> Type -> Type -> Type -> Type -> Region -> Type -> Type

type role STFn9 representational representational representational representational representational representational representational representational representational nominal representational

foreign import data STFn10 :: Type -> Type -> Type -> Type -> Type -> Type -> Type -> Type -> Type -> Type -> Region -> Type -> Type

type role STFn10 representational representational representational representational representational representational representational representational representational representational nominal representational

mkSTFn1 :: forall a t r. (a -> ST t r) -> STFn1 a t r
mkSTFn1 a0 = mkSTFn1 a0
mkSTFn2 :: forall a b t r. (a -> b -> ST t r) -> STFn2 a b t r
mkSTFn2 a0 = mkSTFn2 a0
mkSTFn3 :: forall a b c t r. (a -> b -> c -> ST t r) -> STFn3 a b c t r
mkSTFn3 a0 = mkSTFn3 a0
mkSTFn4 :: forall a b c d t r. (a -> b -> c -> d -> ST t r) -> STFn4 a b c d t r
mkSTFn4 a0 = mkSTFn4 a0
mkSTFn5 :: forall a b c d e t r. (a -> b -> c -> d -> e -> ST t r) -> STFn5 a b c d e t r
mkSTFn5 a0 = mkSTFn5 a0
mkSTFn6 :: forall a b c d e f t r. (a -> b -> c -> d -> e -> f -> ST t r) -> STFn6 a b c d e f t r
mkSTFn6 a0 = mkSTFn6 a0
mkSTFn7 :: forall a b c d e f g t r. (a -> b -> c -> d -> e -> f -> g -> ST t r) -> STFn7 a b c d e f g t r
mkSTFn7 a0 = mkSTFn7 a0
mkSTFn8 :: forall a b c d e f g h t r. (a -> b -> c -> d -> e -> f -> g -> h -> ST t r) -> STFn8 a b c d e f g h t r
mkSTFn8 a0 = mkSTFn8 a0
mkSTFn9 :: forall a b c d e f g h i t r. (a -> b -> c -> d -> e -> f -> g -> h -> i -> ST t r) -> STFn9 a b c d e f g h i t r
mkSTFn9 a0 = mkSTFn9 a0
mkSTFn10 :: forall a b c d e f g h i j t r. (a -> b -> c -> d -> e -> f -> g -> h -> i -> j -> ST t r) -> STFn10 a b c d e f g h i j t r
mkSTFn10 a0 = mkSTFn10 a0

runSTFn1 :: forall a t r. STFn1 a t r -> a -> ST t r
runSTFn1 a0 a1 = runSTFn1 a0 a1
runSTFn2 :: forall a b t r. STFn2 a b t r -> a -> b -> ST t r
runSTFn2 a0 a1 a2 = runSTFn2 a0 a1 a2
runSTFn3 :: forall a b c t r. STFn3 a b c t r -> a -> b -> c -> ST t r
runSTFn3 a0 a1 a2 a3 = runSTFn3 a0 a1 a2 a3
runSTFn4 :: forall a b c d t r. STFn4 a b c d t r -> a -> b -> c -> d -> ST t r
runSTFn4 a0 a1 a2 a3 a4 = runSTFn4 a0 a1 a2 a3 a4
runSTFn5 :: forall a b c d e t r. STFn5 a b c d e t r -> a -> b -> c -> d -> e -> ST t r
runSTFn5 a0 a1 a2 a3 a4 a5 = runSTFn5 a0 a1 a2 a3 a4 a5
runSTFn6 :: forall a b c d e f t r. STFn6 a b c d e f t r -> a -> b -> c -> d -> e -> f -> ST t r
runSTFn6 a0 a1 a2 a3 a4 a5 a6 = runSTFn6 a0 a1 a2 a3 a4 a5 a6
runSTFn7 :: forall a b c d e f g t r. STFn7 a b c d e f g t r -> a -> b -> c -> d -> e -> f -> g -> ST t r
runSTFn7 a0 a1 a2 a3 a4 a5 a6 a7 = runSTFn7 a0 a1 a2 a3 a4 a5 a6 a7
runSTFn8 :: forall a b c d e f g h t r. STFn8 a b c d e f g h t r -> a -> b -> c -> d -> e -> f -> g -> h -> ST t r
runSTFn8 a0 a1 a2 a3 a4 a5 a6 a7 a8 = runSTFn8 a0 a1 a2 a3 a4 a5 a6 a7 a8
runSTFn9 :: forall a b c d e f g h i t r. STFn9 a b c d e f g h i t r -> a -> b -> c -> d -> e -> f -> g -> h -> i -> ST t r
runSTFn9 a0 a1 a2 a3 a4 a5 a6 a7 a8 a9 = runSTFn9 a0 a1 a2 a3 a4 a5 a6 a7 a8 a9
runSTFn10 :: forall a b c d e f g h i j t r. STFn10 a b c d e f g h i j t r -> a -> b -> c -> d -> e -> f -> g -> h -> i -> j -> ST t r
runSTFn10 a0 a1 a2 a3 a4 a5 a6 a7 a8 a9 a10 = runSTFn10 a0 a1 a2 a3 a4 a5 a6 a7 a8 a9 a10

-- | The primitive surface the rest of the library and the corpus build on.
-- |
-- | The class hierarchy is the official `purescript-prelude` v6.0.1 re-export
-- | list. `Effect` stays in this module: `check_run_effect_scope` resolves
-- | `Prelude.runEffect`, and `psrs_core::effect::operations` synthesizes
-- | `effectPure`, `effectBind`, `runEffect`, and `trap` from the `psrs:effect`
-- | bindings declared here. The class methods `pure` and `bind` are the
-- | official `Applicative` and `Bind` methods; the `Effect` instances call
-- | those bindings, so creating an action still does not run it.
-- |
-- | `unit` is not declared here. `Unit` is a builtin, re-exported through
-- | `Data.Unit`, and the one `Unit` value is `Intrinsic::Unit`.
module Prelude
  ( Effect
  , runEffect
  , trap
  , module Control.Applicative
  , module Control.Apply
  , module Control.Bind
  , module Control.Category
  , module Control.Monad
  , module Control.Semigroupoid
  , module Data.Boolean
  , module Data.BooleanAlgebra
  , module Data.Bounded
  , module Data.CommutativeRing
  , module Data.DivisionRing
  , module Data.Eq
  , module Data.EuclideanRing
  , module Data.Field
  , module Data.Function
  , module Data.Functor
  , module Data.HeytingAlgebra
  , module Data.Monoid
  , module Data.NaturalTransformation
  , module Data.Ord
  , module Data.Ordering
  , module Data.Ring
  , module Data.Semigroup
  , module Data.Semiring
  , module Data.Show
  , module Data.Unit
  , module Data.Void
  ) where

import Control.Applicative (class Applicative, pure, liftA1, unless, when)
import Control.Apply (class Apply, apply, (*>), (<*), (<*>))
import Control.Bind (class Bind, bind, class Discard, discard, ifM, join, (<=<), (=<<), (>=>), (>>=))
import Control.Category (class Category, identity)
import Control.Monad (class Monad, liftM1, unlessM, whenM, ap)
import Control.Semigroupoid (class Semigroupoid, compose, (<<<), (>>>))

import Data.Boolean (otherwise)
import Data.BooleanAlgebra (class BooleanAlgebra)
import Data.Bounded (class Bounded, bottom, top)
import Data.CommutativeRing (class CommutativeRing)
import Data.DivisionRing (class DivisionRing, recip)
import Data.Eq (class Eq, eq, notEq, (/=), (==))
import Data.EuclideanRing (class EuclideanRing, degree, div, mod, (/), gcd, lcm)
import Data.Field (class Field)
import Data.Function (const, flip, ($), (#))
import Data.Functor (class Functor, flap, map, void, ($>), (<#>), (<$), (<$>), (<@>))
import Data.HeytingAlgebra (class HeytingAlgebra, conj, disj, not, (&&), (||))
import Data.Monoid (class Monoid, mempty)
import Data.NaturalTransformation (type (~>))
import Data.Ord (class Ord, compare, (<), (<=), (>), (>=), comparing, min, max, clamp, between)
import Data.Ordering (Ordering(..))
import Data.Ring (class Ring, negate, sub, (-))
import Data.Semigroup (class Semigroup, append, (<>))
import Data.Semiring (class Semiring, add, mul, one, zero, (*), (+))
import Data.Show (class Show, show)
import Data.Unit (Unit, unit)
import Data.Void (Void, absurd)

foreign import data Effect :: Type -> Type

-- | Builds an `Effect` that returns `value`. Lowering replaces this binding;
-- | the `Applicative` instance is what user code calls `pure`.
foreign import "psrs:effect#pure" effectPure :: forall a. a -> Effect a

-- | Sequences two effects. The `Bind` instance is what user code calls `bind`.
foreign import "psrs:effect#bind" effectBind :: forall a b. Effect a -> (a -> Effect b) -> Effect b

foreign import "psrs:effect#run" runEffect :: forall a. Effect a -> a

-- | The effect that escapes instead of returning. An uncaught failure on this
-- | target is a guest trap, so this is the one operation whose result never
-- | exists; it is how a library reports an assertion that did not hold.
foreign import "psrs:effect#trap" trap :: Effect Unit

instance functorEffect :: Functor Effect where
  map f action = bind action (\value -> pure (f value))

instance applyEffect :: Apply Effect where
  apply wrapped action =
    bind wrapped (\function -> bind action (\value -> pure (function value)))

instance applicativeEffect :: Applicative Effect where
  pure value = effectPure value

instance bindEffect :: Bind Effect where
  bind action next = effectBind action next

instance monadEffect :: Monad Effect

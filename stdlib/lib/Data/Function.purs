-- | Function combinators and application operators.
-- |
-- | The operators are aliases for the value functions here, exactly as the
-- | official `Data.Function` declares them: `$` applies a function to an
-- | argument, so it is the definition that names `apply`, and the fixity
-- | declaration gives the operator its right associativity and lowest
-- | precedence.
-- |
-- | `identity` is **not** here. The official `Prelude` re-exports it from
-- | `Control.Category`, where it is the `Category` class method, and defining a
-- | private `identity` here first would be the narrower model that has to be
-- | replaced when the class hierarchy lands in #94.
module Data.Function
  ( apply
  , applyFlipped
  , const
  , flip
  , on
  , (#)
  , ($)
  ) where

-- | Applies a function to an argument: `apply f x = f x`.
apply :: forall a b. (a -> b) -> a -> b
apply f x = f x

-- | Applies a function to an argument, with the argument first:
-- | `applyFlipped x f = f x`.
applyFlipped :: forall a b. a -> (a -> b) -> b
applyFlipped x f = f x

-- | Returns its first argument and ignores the second.
const :: forall a b. a -> b -> a
const value _ = value

-- | Reverses the argument order of a two-argument function.
flip :: forall a b c. (a -> b -> c) -> b -> a -> c
flip f b a = f a b

-- | Applies a unary function to both arguments before a binary function:
-- | `on f g x y = f (g x) (g y)`.
on :: forall a b c. (b -> b -> c) -> (a -> b) -> a -> a -> c
on f g x y = f (g x) (g y)

infixr 0 apply as $

infixl 1 applyFlipped as #

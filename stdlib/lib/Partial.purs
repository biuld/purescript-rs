-- | Some partial helper functions. See the README for more documentation.
module Partial
  ( crash
  , crashWith
  ) where

-- | A partial function which crashes on any input with a default message.
crash :: forall a. Partial => a
crash = crashWith "Partial.crash: partial function"

-- | A partial function which crashes on any input with the specified message.
crashWith :: forall a. Partial => String -> a
crashWith = _crashWith

_crashWith :: forall a. String -> a
_crashWith a0 = _crashWith a0

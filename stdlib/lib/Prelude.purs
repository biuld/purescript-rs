
module Prelude where

data Effect a

pure :: forall a. a -> Effect a
pure value = \token -> value

bind :: forall a b. Effect a -> (a -> Effect b) -> Effect b
bind first next = \token -> next (first token) token

runEffect :: forall a. Effect a -> a
runEffect action = action 0

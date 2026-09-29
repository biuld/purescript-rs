
module Prelude where

data Effect a

pure :: forall a. a -> Effect a
pure value = \token -> value

bind :: forall a b. Effect a -> (a -> Effect b) -> Effect b
bind first next = \token -> next (first token) token

discard :: forall a b. Effect a -> (a -> Effect b) -> Effect b
discard first next = bind first next

map :: forall a b. (a -> b) -> Effect a -> Effect b
map f x = bind x (\v -> pure (f v))

apply :: forall a b. Effect (a -> b) -> Effect a -> Effect b
apply f x = bind f (\g -> bind x (\v -> pure (g v)))

runEffect :: forall a. Effect a -> a
runEffect action = action 0

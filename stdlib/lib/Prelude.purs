module Prelude where

foreign import data Effect :: Type -> Type

foreign import "psrs:effect#pure" pure :: forall a. a -> Effect a

foreign import "psrs:effect#bind" bind :: forall a b. Effect a -> (a -> Effect b) -> Effect b

discard :: forall a b. Effect a -> (a -> Effect b) -> Effect b
discard first next = bind first next

map :: forall a b. (a -> b) -> Effect a -> Effect b
map f x = bind x (\v -> pure (f v))

apply :: forall a b. Effect (a -> b) -> Effect a -> Effect b
apply f x = bind f (\g -> bind x (\v -> pure (g v)))

foreign import "psrs:effect#run" runEffect :: forall a. Effect a -> a

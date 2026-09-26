
module Data.Either where

import Data.Maybe

data Either a b = Left a | Right b

either :: forall a b c. (a -> c) -> (b -> c) -> Either a b -> c
either onLeft onRight value = case value of
  Left left -> onLeft left
  Right right -> onRight right

hush :: forall a b. Either a b -> Maybe b
hush value = case value of
  Left _ -> Nothing
  Right right -> Just right

note :: forall a b. a -> Maybe b -> Either a b
note fallback value = case value of
  Nothing -> Left fallback
  Just inner -> Right inner

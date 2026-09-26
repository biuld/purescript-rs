
module Data.Maybe where

data Maybe a = Nothing | Just a

maybe :: forall a b. b -> (a -> b) -> Maybe a -> b
maybe fallback transform value = case value of
  Nothing -> fallback
  Just inner -> transform inner

fromMaybe :: forall a. a -> Maybe a -> a
fromMaybe fallback value = case value of
  Nothing -> fallback
  Just inner -> inner

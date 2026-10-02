module PatternMatrix where

choose input = case input of
  whole@{ outer: { value: inner, enabled: true }, items: [first, 0] } -> inner
  _ -> 0

typed = \(input :: Int) -> case input of
  1 -> input
  _ -> 0

character = case 'x' of
  'x' -> 1
  _ -> 0

module PatternGuardFirstWins where

data Pair = Pair Int String

select :: Pair -> Int
select value | Pair x x <- value = x
select _ = 0

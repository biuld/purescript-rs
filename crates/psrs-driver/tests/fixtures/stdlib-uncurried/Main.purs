module Main where
import Data.Function.Uncurried as S

c0 :: Int
c0 = S.runFn0 (S.mkFn0 (\_ -> 42))

c1 :: Int
c1 = S.runFn2 (S.mkFn2 (\a1 a2 -> intAdd (a1) a2)) 1 2

c2 :: Int
c2 = S.runFn3 (S.mkFn3 (\a1 a2 a3 -> intAdd (intAdd (a1) a2) a3)) 1 2 3

c3 :: Int
c3 = S.runFn4 (S.mkFn4 (\a1 a2 a3 a4 -> intAdd (intAdd (intAdd (a1) a2) a3) a4)) 1 2 3 4

c4 :: Int
c4 = S.runFn5 (S.mkFn5 (\a1 a2 a3 a4 a5 -> intAdd (intAdd (intAdd (intAdd (a1) a2) a3) a4) a5)) 1 2 3 4 5

c5 :: Int
c5 = S.runFn6 (S.mkFn6 (\a1 a2 a3 a4 a5 a6 -> intAdd (intAdd (intAdd (intAdd (intAdd (a1) a2) a3) a4) a5) a6)) 1 2 3 4 5 6

c6 :: Int
c6 = S.runFn7 (S.mkFn7 (\a1 a2 a3 a4 a5 a6 a7 -> intAdd (intAdd (intAdd (intAdd (intAdd (intAdd (a1) a2) a3) a4) a5) a6) a7)) 1 2 3 4 5 6 7

c7 :: Int
c7 = S.runFn8 (S.mkFn8 (\a1 a2 a3 a4 a5 a6 a7 a8 -> intAdd (intAdd (intAdd (intAdd (intAdd (intAdd (intAdd (a1) a2) a3) a4) a5) a6) a7) a8)) 1 2 3 4 5 6 7 8

c8 :: Int
c8 = S.runFn9 (S.mkFn9 (\a1 a2 a3 a4 a5 a6 a7 a8 a9 -> intAdd (intAdd (intAdd (intAdd (intAdd (intAdd (intAdd (intAdd (a1) a2) a3) a4) a5) a6) a7) a8) a9)) 1 2 3 4 5 6 7 8 9

c9 :: Int
c9 = S.runFn10 (S.mkFn10 (\a1 a2 a3 a4 a5 a6 a7 a8 a9 a10 -> intAdd (intAdd (intAdd (intAdd (intAdd (intAdd (intAdd (intAdd (intAdd (a1) a2) a3) a4) a5) a6) a7) a8) a9) a10)) 1 2 3 4 5 6 7 8 9 10

apply2 :: forall a b c. S.Fn2 a b c -> a -> b -> c
apply2 f a b = S.runFn2 f a b
c10 :: Int
c10 = apply2 (S.mkFn2 (\a b -> intAdd a b)) 40 2

main =
  if (booleanAnd (booleanAnd (booleanAnd (intEq c0 42) (intEq c1 3)) (booleanAnd (intEq c2 6) (booleanAnd (intEq c3 10) (intEq c4 15)))) (booleanAnd (booleanAnd (intEq c5 21) (booleanAnd (intEq c6 28) (intEq c7 36))) (booleanAnd (intEq c8 45) (booleanAnd (intEq c9 55) (intEq c10 42))))) then 42 else 1

module Main where
import Prim.Control as C
data F x = F Int (Int -> x)
data G x = G (Int -> x)
type Answer = { saved :: Int -> C.Control F Int, built :: C.Control F Int, value :: Int }
ret :: Int -> C.Control G Answer
ret n = C.return { saved: \x -> C.return x, built: C.return n, value: n }
inner :: forall x. F x -> (x -> C.Control F Int) -> C.Control G Answer
inner (F label k) raw = if intEq label 1
  then C.bind (C.perform (G (\x -> x))) (\n -> loop (raw (k n)))
  else C.return { saved: \n -> raw (k n), built: raw (k 42), value: 21 }
loop :: C.Control F Int -> C.Control G Answer
loop = C.handle ret inner
forward :: forall x. G x -> (x -> C.Control G Answer) -> C.Control G Answer
forward request raw = C.bind (C.perform request) (\answer -> wrap (raw answer))
wrap :: C.Control G Answer -> C.Control G Answer
wrap computation = C.handle C.return forward computation
root :: forall x. Int -> G x -> (x -> C.Control G Answer) -> { _1 :: Int, _2 :: C.Control G Answer }
root s (G k) raw = { _1: s, _2: C.bind (raw (k 0)) (\a -> AFTER) }
closed :: forall x. Int -> F x -> (x -> C.Control F Int) -> { _1 :: Int, _2 :: C.Control F Int }
closed s (F label k) raw = { _1: s, _2: raw (k 42) }
program = C.bind (C.perform (F 1 (\x -> x))) (\_ -> C.perform (F 2 (\x -> x)))
main = (C.run 0 root (wrap (loop program)))._2.value

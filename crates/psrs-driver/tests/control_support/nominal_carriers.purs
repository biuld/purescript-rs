module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
data Box a = Box a
unbox :: forall a. Box a -> a
unbox (Box value) = value
keep :: forall f a. C.Control f a -> C.Control f a
keep value = value
retain :: forall f x a. C.Resumption f x a -> C.Resumption f x a
retain value = value
apply :: forall a b. (a -> b) -> a -> b
apply function value = function value
choose :: forall f a. Boolean -> C.Control f a -> C.Control f a -> C.Control f a
choose flag left right = if flag then left else right
clause :: forall x. Op x -> (x -> C.Control Op Int) -> C.Control Op Int
clause (Op k) raw = raw (k 40)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler state (Op k) raw = { _1: state, _2: raw (k 0) }
main :: Int
main = let owner = retain (apply unbox (Box (C.once (\n -> C.return (intAdd n 2)))))
           spare = C.once (\n -> C.return n)
           cancelled = C.cancel spare
           saved = { owner: owner, computation: C.perform (Op \x -> x) }
           program = C.bind (choose true (apply keep saved.computation) (C.return 0)) (\n -> C.resume saved.owner n)
       in (C.run 0 handler (C.handle (\n -> C.return n) clause program))._2

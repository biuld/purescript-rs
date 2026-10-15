pub(super) const DEEP: &str = r"module Main where
import Prim.Control as C
data Op x = Op (Int -> x)
deep :: Int -> C.Control Op Int -> C.Control Op { count :: Int, value :: Int }
deep count body = C.handle
  (\value -> C.return { count: count, value: value })
  (\(Op k) raw -> deep (intAdd count 1) (raw (k 0)))
  body
root :: forall x. Int -> Op x -> (x -> C.Control Op { count :: Int, value :: Int }) -> { _1 :: Int, _2 :: C.Control Op { count :: Int, value :: Int } }
root s (Op k) raw = { _1: s, _2: raw (k 0) }
build :: Int -> C.Control Op Int
build n = if intEq n 0 then C.return 42 else C.bind (C.perform (Op (\x -> x))) (\_ -> build (intSub n 1))
result = (C.run 0 root (deep 0 (build STEPS)))._2
main = if intEq result.count STEPS then result.value else 0
";

pub(super) const FORWARDED: &str = r"module Main where
import Prim.Control as C
data F x = F (Int -> x)
data G x = G (Int -> x)
data U x = UF (F x) | UG (G x)
fold :: Int -> C.Control U Int -> C.Control G { count :: Int, value :: Int }
fold count body = C.handle
  (\value -> C.return { count: count, value: value })
  (\request raw -> case request of
    UF (F k) -> fold (intAdd count 1) (raw (k 0))
    UG op -> C.bind (C.perform op) (\answer -> fold count (raw answer)))
  body
root :: forall x. Int -> G x -> (x -> C.Control G { count :: Int, value :: Int }) -> { _1 :: Int, _2 :: C.Control G { count :: Int, value :: Int } }
root s (G k) raw = { _1: intAdd s 1, _2: raw (k 0) }
build :: Int -> C.Control U Int
build n = if intEq n 0 then C.return 42 else C.bind (C.perform (UF (F (\x -> x)))) (\_ -> C.bind (C.perform (UG (G (\x -> x)))) (\_ -> build (intSub n 1)))
result = C.run 0 root (fold 0 (build STEPS))
main = if intEq result._1 STEPS then if intEq result._2.count STEPS then result._2.value else 0 else 0
";

pub(super) const NESTED: &str = r"module Main where
import Prim.Control as C
data Op x = Op (Int -> x)
clause :: forall x. Op x -> (x -> C.Control Op Int) -> C.Control Op Int
clause (Op k) raw = raw (k 0)
nest :: Int -> C.Control Op Int
nest n = if intEq n 0 then C.return 42 else C.bind (C.handle C.return clause (nest (intSub n 1))) (\n -> C.return n)
root :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
root s (Op k) raw = { _1: s, _2: raw (k 0) }
main = (C.run 0 root (nest STEPS))._2
";

/// One request with LENGTH callbacks already in the captured sequence.
/// Pass the run result once to `check`: separate top-level projections would
/// evaluate this pure definition twice and double the measured machine work.
pub(super) const SAVED_LENGTH: &str = r"module Main where
import Prim.Control as C
data Op x = Op (Int -> x)
step :: Int -> Int -> Int
step index value = intAdd (intMul value 3) index
expected :: Int -> Int
expected n = if intEq n 0 then 42 else step n (expected (intSub n 1))
build :: Int -> C.Control Op Int
build n = if intEq n 0 then C.perform (Op (\x -> x)) else C.bind (build (intSub n 1)) (\value -> C.return (step n value))
root :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
root s (Op k) raw = { _1: intAdd s 1, _2: raw (k 42) }
result = C.run 0 root (build LENGTH)
check :: { _1 :: Int, _2 :: Int } -> Int
check result = if intEq result._1 1 then if intEq result._2 (expected LENGTH) then 42 else 0 else 0
main = check result
";

/// The first replay finishes before building the second request's sequence,
/// so the second capture is a linear Cons chain with LENGTH callbacks.
pub(super) const LINEAR_SECOND: &str = r"module Main where
import Prim.Control as C
data Op x = Op (Int -> x)
step :: Int -> Int -> Int
step index value = intAdd (intMul value 3) index
expected :: Int -> Int
expected n = if intEq n 0 then 42 else step n (expected (intSub n 1))
build :: Int -> C.Control Op Int
build n = if intEq n 0 then C.perform (Op (\x -> x)) else C.bind (build (intSub n 1)) (\value -> C.return (step n value))
root :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
root s (Op k) raw = if intEq s 0 then { _1: 1, _2: C.bind (raw (k 0)) (\_ -> build LENGTH) } else { _1: intAdd s 1, _2: raw (k 42) }
result = C.run 0 root (C.perform (Op (\x -> x)))
check :: { _1 :: Int, _2 :: Int } -> Int
check result = if intEq result._1 2 then if intEq result._2 (expected LENGTH) then 42 else 0 else 0
main = check result
";

/// After the first replay enters its first callback, the second capture sees
/// Append(remainingCallbacks, Empty): same callbacks as LINEAR_SECOND.
pub(super) const APPEND_SECOND: &str = r"module Main where
import Prim.Control as C
data Op x = Op (Int -> x)
step :: Int -> Int -> Int
step index value = intAdd (intMul value 3) index
expected :: Int -> Int
expected n = if intEq n 0 then 42 else step n (expected (intSub n 1))
build :: Int -> C.Control Op Int
build n = if intEq n 0 then C.bind (C.perform (Op (\x -> x))) (\_ -> C.perform (Op (\x -> x))) else C.bind (build (intSub n 1)) (\value -> C.return (step n value))
root :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
root s (Op k) raw = { _1: intAdd s 1, _2: raw (k 42) }
result = C.run 0 root (build LENGTH)
check :: { _1 :: Int, _2 :: Int } -> Int
check result = if intEq result._1 2 then if intEq result._2 (expected LENGTH) then 42 else 0 else 0
main = check result
";

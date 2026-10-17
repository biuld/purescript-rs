pub const RIGID_RESULT: &str = r#"module Main where
use :: forall r. (forall a. a -> r) -> r
use f = f 0
bad :: Int
bad = use (\value -> value)
main :: Int
main = 42
"#;

pub const RIGID_NOMINAL_RESULT: &str = r#"module Main where
data Region :: Type -> Type -> Type
data Region r a = Region a
run :: forall a. (forall r. Region r a) -> a
run (Region value) = value
leak :: forall r. Region r (Region r Int)
leak = Region (Region 0)
main :: Int
main = run leak
"#;

pub const CONSTANT_RESULT: &str = r#"module Main where
use :: forall r. (forall a. a -> r) -> r
use f = f 0
result :: Int
result = use (\_ -> 42)
main :: Int
main = result
"#;

pub const RETURNED_EVIDENCE: &str = r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
make :: Int -> (Mark Int => Int)
make _ = mark 42
result :: Int
result = make 0
main :: Int
main = result
"#;

pub const LAMBDA_RESULT: &str = r#"module Main where
class Default a where
  def :: a
instance Default Int where
  def = 42
data Pair a = Pair a a
make :: forall a b. Default b => Pair (a -> b)
make = Pair left right where
  left _ = def
  right _ = def
main :: Int
main = case (make :: Pair (Int -> Int)) of
  Pair left right -> right (left 0)
"#;

pub const CONSTRUCTOR_SCRUTINEE: &str = r#"module Main where
class Default a where
  def :: a
instance Default Int where
  def = 42
data I a = I a
instance Default a => Default (I a) where
  def = I def
main :: Int
main = case def of
  I value -> value
"#;

pub const LITERAL_SCRUTINEE: &str = r#"module Main where
class Default a where
  def :: a
instance Default Int where
  def = 42
main :: Int
main = case def of
  42 -> 42
  _ -> 1
"#;

pub const VISIBLE_VARIABLE_SCRUTINEE: &str = r#"module Main where
identity :: forall @a. a -> a
identity value = value
main :: Int
main = case identity of
  retained -> if retained true then retained 42 else 1
"#;

use super::*;

const ROW_CONTEXT: &str = r"module Main where
import Prim.Row as R
import Prim.RowList as RL
class FoldableVFRL :: RL.RowList (Type -> Type) -> Row (Type -> Type) -> Constraint
class FoldableVFRL rl row | rl -> row
instance foldableCons ::
  ( FoldableVFRL rl r, R.Cons k f r r' ) => FoldableVFRL (RL.Cons k f rl) r'
class TraversableVFRL :: RL.RowList (Type -> Type) -> Row (Type -> Type) -> Constraint
class FoldableVFRL rl row <= TraversableVFRL rl row | rl -> row
instance traversableCons ::
  ( TraversableVFRL rl r, R.Cons k f r r', R.Union r rx r' ) => TraversableVFRL (RL.Cons k f rl) r'
main :: Int
main = 42
";

#[test]
fn nested_instance_context_improves_from_a_given_superclass_row_dependency() {
    check_source("Main.purs", ROW_CONTEXT).expect("the superclass determines the residual row");
}

#[test]
fn nested_instance_context_without_a_residual_row_dictionary_is_rejected() {
    let source = ROW_CONTEXT.replace("TraversableVFRL rl r, R.Cons", "R.Cons");
    let errors = check_source("Main.purs", &source).expect_err("no residual dictionary exists");
    assert!(
        errors
            .iter()
            .any(|error| error.code == Some("NoInstanceFound")),
        "{errors:?}"
    );
}

#[test]
fn nested_instance_context_improvement_executes_user_class_dictionaries() {
    let source = r"module Main where
data Wrapper a = Wrapper a
class Describe a b | a -> b where
  describe :: a -> b
class Value a where
  value :: a -> Int
instance describeInt :: Describe Int Int where
  describe x = x
instance valueInt :: Value Int where
  value x = x
instance valueWrapper :: (Describe a b, Value b) => Value (Wrapper a) where
  value (Wrapper x) = value (describe x)
useGiven :: forall a b. Describe a b => Value b => Wrapper a -> Int
useGiven wrapped = value wrapped
main :: Int
main = useGiven (Wrapper 42)
";
    let output = run_with_wasmtime(source).expect("required Wasmtime runtime");
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

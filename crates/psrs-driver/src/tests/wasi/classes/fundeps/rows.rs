use super::*;

const SOURCE: &str = r"module Main where
import Type.Equality (class TypeEquals)
import Type.Proxy (Proxy)
import Unsafe.Coerce (unsafeCoerce)
data R r a = R a
fromRows :: forall f r1 r2 a. TypeEquals (Proxy r1) (Proxy r2) => f r2 a -> f r1 a
fromRows = unsafeCoerce
inject :: forall r a. a -> R (value :: Array | r) a
inject = R
class Lift m where
  lift :: forall a. a -> m a
instance liftR :: TypeEquals (Proxy r1) (Proxy (value :: Array | r2)) => Lift (R r1) where
  lift value = fromRows (inject value)
class Lift m <= Child m where
  stamp :: m Int -> Int
instance childR :: TypeEquals (Proxy r1) (Proxy (other :: Array, value :: Array | r2)) => Child (R r1) where
  stamp (R n) = n
main = stamp (R 42 :: R (other :: Array, value :: Array) Int)
";

#[test]
fn superclass_context_refines_a_row_tail_from_a_given_dependency() {
    let Some(output) = run_with_wasmtime(SOURCE) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn superclass_context_does_not_invent_a_missing_row_field() {
    let source = SOURCE.replace("other :: Array, value :: Array | r2", "other :: Array | r2");
    let errors = compile_source("Main.purs", &source)
        .expect_err("an abstract tail cannot guarantee a missing field");
    assert!(
        errors
            .iter()
            .any(|error| error.code == Some("TypesDoNotUnify")),
        "{errors:?}"
    );
}

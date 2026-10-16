use super::*;

#[test]
fn higher_order_alias_arguments_are_substituted_before_elaboration() {
    let source = r"module Main where
type Apply f x = f x
type Ignore f = Int
type Wrap a b = { value :: a, other :: b }
value :: Apply (Apply (Wrap Int)) Boolean
value = { value: 42, other: true }
main :: Ignore (Wrap Int)
main = value.value
";
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn open_row_alias_operator_preserves_the_row_tail() {
    let source = r"module Main where
type RowApply f a = f a
infixr 0 type RowApply as +
type VALUE r = (value :: Int | r)
read :: forall r. Record (VALUE + r) -> Int
read row = row.value
main = read { value: 42, other: true }
";
    typecheck_program_sources(&[("Main.purs", source)])
        .expect("the open tail survives kind checking and elaboration");
    let closed = source.replace(
        "forall r. Record (VALUE + r)",
        "Record (VALUE + (other :: Boolean))",
    );
    let Some(output) = run_with_wasmtime(&closed) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

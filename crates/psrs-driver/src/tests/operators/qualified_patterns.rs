use super::*;

#[test]
fn qualified_constructor_operator_patterns_reassociate_and_execute() {
    let library = (
        "Chain.purs",
        r"module Chain where
data Chain a = Link a (Chain a) | Empty
infixr 6 Link as :
data Wrapped = Wrap Int
",
    );
    let main = (
        "Main.purs",
        r"module Main where
import Chain as Foo.Bar
values = Foo.Bar.Wrap 20 Foo.Bar.: Foo.Bar.Wrap 22 Foo.Bar.: Foo.Bar.Empty
main :: Int
main = case values of
  Foo.Bar.Wrap first Foo.Bar.: Foo.Bar.Wrap second Foo.Bar.: _ -> intAdd first second
  _ -> 0
",
    );
    let output = run_program_with_wasmtime(&[library, main]).expect("required Wasmtime runtime");
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

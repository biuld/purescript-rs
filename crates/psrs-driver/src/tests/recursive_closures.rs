use super::*;

#[test]
fn recursive_local_function_can_return_a_branch_selected_closure() {
    let source = r"module Main where
import Prelude
main = go 42 0
  where
  go n = if n == 0 then (\a -> a) else (\a -> go (n - 1) (a + 1))
";
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn recursive_local_aliases_share_the_same_capture_environment() {
    let source = r"module Main where
import Prelude
main = outer 1
  where
  outer step = go 42 0
    where
    alias = go
    go n a = if n == 0 then a else alias (n - 1) (a + step)
";
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

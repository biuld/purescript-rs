use super::*;

#[test]
fn backticked_sections_keep_operand_order_and_resolve_local_values() {
    let source = r#"
module Main where
subtract :: Int -> Int -> Int
subtract x y = intSub x y
main :: Int
main = let left = (_ `subtract` 8)
           right = (50 `subtract` _)
           local x y = intSub x y
       in if intEq (left 50) 42
          then if intEq (right 8) 42 then (_ `local` 8) 50 else 0
          else 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

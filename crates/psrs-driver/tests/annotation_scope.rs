use std::process::Command;

const EVALUATION_ORDER: &str = r#"module Main where
import Prelude
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)

main :: Int
main = unsafePerformEffect do
  history <- Ref.new 0
  Ref.modify_ (\value -> value * 10 + 1) history
  let identity = ((unsafePerformEffect do
        Ref.modify_ (\value -> value * 10 + 2) history
        pure (\value -> (value :: b))) :: forall b. b -> b) :: forall a. a -> a
  Ref.modify_ (\value -> value * 10 + 3) history
  let result = if identity true then identity 42 else 1
  observed <- Ref.read history
  pure (if observed == 123 then result else 1)
"#;

#[test]
fn an_annotation_adapter_evaluates_its_value_once_in_source_order() {
    let artifact =
        psrs_driver::compile_program_sources_with_prelude(&[("Main.purs", EVALUATION_ORDER)])
            .expect("annotated value compiles with ordinary Effect.Ref operations");
    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert_ne!(std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(), Ok("1"));
        eprintln!("skipping annotation evaluation order: Wasmtime is unavailable");
        return;
    }
    let path =
        std::env::temp_dir().join(format!("psrs-annotation-order-{}.wasm", std::process::id()));
    std::fs::write(&path, artifact.wasm).expect("write annotation component");
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("execute annotation component");
    assert_eq!(
        output.status.code(),
        Some(42),
        "the value must run once between writes 1 and 3: {output:?}"
    );
    std::fs::remove_file(path).expect("remove annotation component");
}

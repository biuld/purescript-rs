//! Source record fields instantiate names and visible applications, while
//! explicitly annotated polymorphic fields retain their checked boundary.

#[path = "support/record_instantiation.rs"]
mod cases;

#[test]
fn inferred_fields_and_explicit_polymorphic_fields_execute() {
    use std::process::Command;

    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert_ne!(std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(), Ok("1"));
        return;
    }
    for (name, source) in cases::cases() {
        let artifact = psrs_driver::compile_source(name, &source)
            .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        let path = std::env::temp_dir().join(format!(
            "psrs-record-instantiation-{}-{name}.wasm",
            std::process::id()
        ));
        std::fs::write(&path, artifact.wasm).unwrap();
        let output = Command::new("wasmtime").arg(&path).output().unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(output.status.code(), Some(42), "{name}: {output:?}");
    }
}

#[test]
fn inferred_record_fields_discharge_constraints_after_visible_application() {
    let source = r#"module Main where
import Prelude

data Proxy a = Proxy
class Reflect a b | a -> b where
  reflectValue :: Proxy a -> b
instance Reflect Int String where
  reflectValue _ = "value"

reflect :: forall @a b. Reflect a b => b
reflect = reflectValue (Proxy @a)

main :: String
main = show { value: reflect @Int }
"#;
    psrs_driver::check_source("Main.purs", source).expect("instantiate the record field's result");
}

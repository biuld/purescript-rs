#[path = "support/reflectable.rs"]
mod cases;

#[test]
fn reflected_literals_determine_their_term_types_and_execute() {
    use std::process::Command;
    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert_ne!(std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(), Ok("1"));
        return;
    }
    for (name, main) in cases::cases() {
        let artifact = psrs_driver::compile_program_sources(&cases::inputs(&main))
            .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        let path = std::env::temp_dir().join(format!(
            "psrs-reflectable-{}-{name}.wasm",
            std::process::id()
        ));
        std::fs::write(&path, artifact.wasm).unwrap();
        let output = Command::new("wasmtime").arg(&path).output().unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(output.status.code(), Some(42), "{name}: {output:?}");
    }
}

#[test]
fn an_unrelated_class_with_the_same_name_has_no_compiler_rule() {
    let ordinary = cases::REFLECTABLE.replace("module Data.Reflectable", "module Ordinary");
    let main = "module Main where\nimport Ordinary\nimport Type.Proxy (Proxy(..))\nvalue :: String\nvalue = reflectType (Proxy :: Proxy \"hi\")\n";
    let errors = psrs_driver::check_program(&[
        ("Proxy.purs", cases::PROXY),
        ("Ordinary.purs", &ordinary),
        ("Main.purs", main),
    ])
    .expect_err("ordinary classes need source instances");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
        "{errors:?}"
    );
}

#[test]
fn malformed_reflection_contracts_are_rejected() {
    for malformed in [
        cases::REFLECTABLE.replace("| v -> t", ""),
        cases::REFLECTABLE.replace("Proxy v -> t", "Proxy v -> Int"),
        cases::REFLECTABLE.replace("forall k. k -> Type", "Symbol -> Type"),
    ] {
        let errors = psrs_driver::check_program(&[
            ("Proxy.purs", cases::PROXY),
            ("Reflectable.purs", &malformed),
        ])
        .expect_err("a compiler interface must satisfy its declared contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.message.contains("dictionary contract")),
            "{errors:?}"
        );
    }
}

#[test]
fn the_official_visible_reflection_program_typechecks_with_the_locked_package() {
    psrs_driver::check_source(
        "4500.purs",
        include_str!("../../../tests/upstream/passing/4500.purs"),
    )
    .expect("the public Reflectable consumer must typecheck");
}

#[test]
fn a_reflected_integer_must_fit_the_target_int() {
    let main = "module Main where\nimport Data.Reflectable\nimport Type.Proxy (Proxy(..))\nvalue :: Int\nvalue = reflectType (Proxy :: Proxy 2147483648)\n";
    let errors = psrs_driver::check_program(&cases::inputs(main))
        .expect_err("reflection cannot fabricate an overflowing Int literal");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("IntOutOfRange")),
        "{errors:?}"
    );
}

#[path = "support/record_kinds.rs"]
mod cases;

#[test]
fn record_and_row_fields_obey_their_distinct_kinds() {
    psrs_driver::check_program(&cases::sources(cases::MAIN)).expect("record fields fix Type");
    psrs_driver::check_source("Main.purs", cases::ROW_SYMBOL).expect("bare rows remain polykinded");
    for (name, source) in cases::REJECT {
        let errors =
            psrs_driver::check_source(name, source).expect_err("record fields must be types");
        assert!(
            errors
                .iter()
                .any(|error| error.code == Some("KindsDoNotUnify")),
            "{name}: {errors:?}"
        );
    }
}

#[test]
fn imported_record_synonyms_and_the_official_consumer_execute() {
    use std::process::Command;
    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert_ne!(std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(), Ok("1"));
        return;
    }
    for (name, main, expected) in [
        ("imported-record", cases::MAIN, 42),
        (
            "official-4105",
            include_str!("../../../tests/upstream/passing/4105.purs"),
            0,
        ),
    ] {
        let artifact = psrs_driver::compile_program_sources_with_prelude(&cases::sources(main))
            .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        let path = std::env::temp_dir().join(format!(
            "psrs-record-kind-{}-{name}.wasm",
            std::process::id()
        ));
        std::fs::write(&path, artifact.wasm).unwrap();
        let output = Command::new("wasmtime")
            .args(["run", "-W", "max-wasm-stack=524288"])
            .arg(&path)
            .output()
            .unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(output.status.code(), Some(expected), "{name}: {output:?}");
        if name == "official-4105" {
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                "Done",
                "{output:?}"
            );
        }
    }
}

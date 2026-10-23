use std::process::Command;

#[path = "support/inferred_imports.rs"]
mod cases;

#[test]
fn checked_inferred_values_cross_import_boundaries() {
    for (name, sources) in cases::ACCEPT {
        psrs_driver::check_program(sources).unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        let reversed = sources.iter().copied().rev().collect::<Vec<_>>();
        psrs_driver::check_program_types_lenient(&reversed)
            .unwrap_or_else(|errors| panic!("reversed lenient {name}: {errors:?}"));
    }
    let errors = psrs_driver::check_program(cases::MISSING).expect_err("missing instance");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
        "{errors:?}"
    );
}

#[test]
fn imported_inferred_values_execute() {
    if !Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        assert_ne!(std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(), Ok("1"));
        return;
    }
    for (name, sources) in cases::ACCEPT {
        let artifact = psrs_driver::compile_program_sources(sources)
            .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        let path = std::env::temp_dir().join(format!(
            "psrs-inferred-import-{}-{name}.wasm",
            std::process::id()
        ));
        std::fs::write(&path, artifact.wasm).unwrap();
        let output = Command::new("wasmtime")
            .arg("run")
            .arg(&path)
            .output()
            .unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(output.status.code(), Some(42), "{name}: {output:?}");
    }
}

#[test]
fn failed_producers_are_not_checked_interfaces_and_private_values_stay_hidden() {
    assert!(psrs_driver::check_program(cases::PRIVATE).is_err());
    for sources in [
        cases::BAD_PRODUCER.to_vec(),
        cases::BAD_PRODUCER.iter().copied().rev().collect(),
    ] {
        let provider = sources
            .iter()
            .position(|(name, _)| *name == "Provider.purs")
            .unwrap();
        for result in [
            psrs_driver::check_program(&sources),
            psrs_driver::check_program_types_lenient(&sources),
        ] {
            let errors = result.expect_err("a failed producer publishes no interface");
            assert!(
                errors
                    .iter()
                    .any(|error| error.diagnostic.code == Some("TypesDoNotUnify")),
                "{errors:?}"
            );
            assert!(
                errors
                    .iter()
                    .all(|error| error.source == psrs_driver::DiagnosticOrigin::Source(provider)),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn publication_excludes_private_values_and_records_available_source_ranges() {
    let sources = [(
        "Provider.purs",
        "module Provider (public) where\npublic = 42\nprivate = 1\n",
    )];
    let mut modules = psrs_driver::resolve_program_sources(&sources).unwrap();
    let (kinds, errors) = psrs_kind::check_program(&modules);
    assert!(errors.is_empty(), "{errors:?}");
    let module = modules.remove(0);
    let public = module
        .declarations
        .iter()
        .find(|value| value.name == "public")
        .unwrap()
        .clone();
    let private = module
        .declarations
        .iter()
        .find(|value| value.name == "private")
        .unwrap()
        .symbol;
    let module_names = std::collections::HashMap::from([(module.id, module.name.clone())]);
    let output = psrs_typecheck::typecheck_module_with_checked_kinds_and_module_names_and_warnings(
        module,
        &std::collections::HashMap::new(),
        None,
        false,
        psrs_typecheck::TypecheckContext {
            known_types: &[],
            known_values: &[],
            imported_instances: &[],
            module_names: &module_names,
            checked_kinds: &kinds,
            checked_values: &std::collections::HashMap::new(),
        },
    )
    .unwrap();
    assert_eq!(output.values.len(), 1);
    let signature = output.values.get(&public.symbol).unwrap();
    assert_eq!(signature.source_span(), public.span);
    assert_eq!(signature.name_span(), Some(public.name_span));
    assert!(!output.values.contains_key(&private));
}

#[test]
fn source_foreign_schemes_check_but_require_a_real_target_implementation() {
    psrs_driver::check_program(cases::FOREIGN)
        .expect("foreign signature is a checked semantic value");
    psrs_driver::compile_program_sources(cases::FOREIGN)
        .expect_err("a polymorphic foreign signature does not fabricate a target implementation");
    let errors = psrs_driver::check_program(cases::CYCLE).expect_err("cyclic source modules");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("CycleInModules")),
        "{errors:?}"
    );
}

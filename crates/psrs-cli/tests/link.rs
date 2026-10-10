use std::{fs, process::Command};

const API: &str = "test:cli/api@1.0.0";

#[test]
fn link_uses_manifest_relative_pins_and_reports_the_closed_graph() {
    let root = std::env::temp_dir().join(format!("psrs-cli-link-{}", std::process::id()));
    let package = root.join("package");
    fs::create_dir_all(&package).unwrap();
    let application = wat::parse_str(format!(
        r#"(component
      (type $api (instance (export "value" (func (result u32)))))
      (import "{API}" (instance $api (type $api)))
      (export "{API}" (instance $api)))"#
    ))
    .unwrap();
    let provider = wat::parse_str(format!(
        r#"(component
      (core module $m (func (export "value") (result i32) i32.const 42))
      (core instance $m (instantiate $m))
      (func $value (result u32) (canon lift (core func $m "value")))
      (instance $api (export "value" (func $value)))
      (export "{API}" (instance $api)))"#
    ))
    .unwrap();
    let app_path = root.join("app.wasm");
    let manifest_path = package.join("providers.json");
    let output_path = root.join("linked.wasm");
    let report_path = root.join("report.json");
    fs::write(&app_path, &application).unwrap();
    fs::write(package.join("provider.wasm"), &provider).unwrap();
    let mut manifest = serde_json::json!({
      "schema_version":1,"application_sha256":psrs_linker::sha256_hex(&application),
      "providers":[{"id":"provider","path":"provider.wasm","sha256":psrs_linker::sha256_hex(&provider)}],
      "bindings":[{"interface":API,"provider":"provider"}],"permitted_host_interfaces":[],
    });
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_psrs"))
            .arg("link")
            .arg(&app_path)
            .arg("--manifest")
            .arg(&manifest_path)
            .arg("-o")
            .arg(&output_path)
            .arg("--report")
            .arg(&report_path)
            .output()
            .unwrap()
    };
    let result = run();
    assert!(result.status.success(), "{result:?}");
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report["external_world"], serde_json::json!([]));
    assert_eq!(report["component_artifacts"].as_array().unwrap().len(), 2);
    assert_eq!(
        report["output_sha256"],
        psrs_linker::sha256_hex(&fs::read(&output_path).unwrap())
    );
    fs::remove_file(&output_path).unwrap();
    fs::remove_file(&report_path).unwrap();
    manifest["providers"][0]["sha256"] = "0".repeat(64).into();
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let result = run();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("digest"));
    assert!(!output_path.exists());
    assert!(!report_path.exists());
    manifest["providers"][0]["sha256"] = psrs_linker::sha256_hex(&provider).into();
    manifest["permitted_host_interfaces"] =
        serde_json::json!(["wasi:http/outgoing-handler@0.2.12"]);
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let result = run();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("target profile"));
    assert!(!output_path.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn source_build_composes_a_guest_and_joins_the_exact_artifact_lineage() {
    let root = std::env::temp_dir().join(format!("psrs-cli-build-link-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let interface = "wasi:random/insecure-seed@0.2.12";
    let provider = wat::parse_str(format!(
        r#"(component
      (core module $m
        (memory (export "memory") 1)
        (func (export "insecure-seed") (result i32)
          i32.const 0 i64.const 5 i64.store
          i32.const 8 i64.const 37 i64.store i32.const 0))
      (core instance $m (instantiate $m))
      (func $seed (result (tuple u64 u64))
        (canon lift (core func $m "insecure-seed") (memory $m "memory")))
      (instance $api (export "insecure-seed" (func $seed)))
      (export "{interface}" (instance $api)))"#
    ))
    .unwrap();
    fs::write(root.join("seed.wasm"), &provider).unwrap();
    fs::write(root.join("Main.purs"), "module Main where\nimport Prelude\nimport WASI.Random (insecureSeed)\nmain = let seed = runEffect insecureSeed in seed._1 + seed._2\n").unwrap();
    let mut manifest = serde_json::json!({
        "schema_version":1,
        "providers":[{"id":"seed", "path":"seed.wasm", "sha256":psrs_linker::sha256_hex(&provider)}],
        "bindings":[{"interface":interface, "provider":"seed"}],
        "permitted_host_interfaces":["wasi:cli/exit@0.2.12"],
    });
    let manifest_path = root.join("providers.json");
    let output = root.join("linked.wasm");
    let report = root.join("report.json");
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_psrs"))
            .arg("build")
            .arg(root.join("Main.purs"))
            .arg("--manifest")
            .arg(&manifest_path)
            .arg("-o")
            .arg(&output)
            .arg("--report")
            .arg(&report)
            .output()
            .unwrap()
    };
    let result = run();
    assert!(result.status.success(), "{result:?}");
    let evidence: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(
        evidence["composition"]["component_artifacts"][0][1],
        evidence["application_sha256"]
    );
    assert_eq!(
        evidence["composition"]["output_sha256"],
        evidence["output_sha256"]
    );
    assert_eq!(
        evidence["output_sha256"],
        psrs_linker::sha256_hex(&fs::read(&output).unwrap())
    );
    assert_eq!(
        evidence["source_compilation"]["component_output"]["sha256"],
        evidence["application_sha256"]
    );
    let output_id = &evidence["source_compilation"]["component_output"]["artifact"];
    assert!(
        evidence["source_compilation"]["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| &artifact["id"] == output_id
                && artifact["representation"] == "component_binary")
    );
    let executions = evidence["source_compilation"]["executions"]
        .as_array()
        .unwrap();
    assert!(
        executions
            .iter()
            .any(|pass| pass["pass_key"] == "backend.target.plan"),
        "{executions:?}"
    );
    let executed = Command::new("wasmtime")
        .arg("run")
        .arg(&output)
        .output()
        .expect("Wasmtime required");
    assert_eq!(executed.status.code(), Some(42), "{executed:?}");
    assert!(executed.stdout.is_empty());
    assert!(executed.stderr.is_empty());
    fs::remove_file(&output).unwrap();
    fs::remove_file(&report).unwrap();
    manifest["application_sha256"] = "0".repeat(64).into();
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let result = run();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("digest"));
    assert!(!output.exists());
    let rejection: serde_json::Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(rejection["status"], "rejected");
    assert!(
        rejection["composition"]["diagnostic"]
            .as_str()
            .unwrap()
            .contains("digest")
    );
    assert!(rejection.get("output_sha256").is_none());
    fs::remove_dir_all(root).unwrap();
}

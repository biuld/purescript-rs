use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn test_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after UNIX epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("psrs-{label}-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&path).expect("create CLI test directory");
    path
}

fn diagnose(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_psrs"))
        .args(["diagnose"])
        .args(args)
        .output()
        .expect("run psrs diagnose")
}

fn value(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).expect("read JSON fixture"))
        .expect("parse JSON fixture")
}

#[test]
fn diagnose_trace_captures_pass_dumps_and_keeps_v1_status_comparison() {
    let root = test_dir("diagnose-trace");
    let source = root.join("Main.purs");
    let manifest = root.join("manifest.json");
    let traced_manifest = root.join("trace.json");
    let changed_manifest = root.join("changed-trace.json");
    let legacy_manifest = root.join("legacy-v1.json");
    std::fs::write(
        &source,
        "module Main where\nimport Prelude\nmain = 40 + 2\n",
    )
    .expect("write accepted source");

    for (args, label) in [
        (
            vec![
                source.to_str().unwrap(),
                "--out",
                manifest.to_str().unwrap(),
            ],
            "manifest",
        ),
        (
            vec![
                source.to_str().unwrap(),
                "--trace",
                "--out",
                traced_manifest.to_str().unwrap(),
            ],
            "trace",
        ),
    ] {
        let result = diagnose(&args);
        assert!(
            result.status.success(),
            "{label} diagnosis failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    let before = value(&manifest);
    assert_eq!(before["schema_version"], 2);
    assert_eq!(before["trace_mode"], "manifest");
    let before_case = &before["cases"][0];
    assert_eq!(before_case["status"], "passed");
    assert_eq!(before_case["trace"]["dumps_requested"], false);
    assert_eq!(
        before_case["trace"]["canonical_summaries"]["status"],
        "unavailable"
    );
    assert!(before_case["trace"]["executions"].as_array().unwrap().len() > 1);
    let frontend = &before_case["trace"]["executions"][0];
    assert_eq!(frontend["id"], "frontend:e0");
    assert!(
        frontend["inputs"]
            .as_array()
            .unwrap()
            .contains(&"inputs:i0".into())
    );
    assert!(
        frontend["inputs"]
            .as_array()
            .unwrap()
            .contains(&"inputs:trusted_stdlib:i0".into())
    );
    let core = before_case["trace"]["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|artifact| artifact["id"] == "backend:a0")
        .unwrap();
    assert_eq!(core["state"], "produced");
    assert_eq!(core["producer"], "frontend:e0");
    assert!(
        before_case["trace"]["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|artifact| artifact["retained_dump"].is_null())
    );

    let traced = value(&traced_manifest);
    assert_eq!(traced["trace_mode"], "dumps");
    let traced_case = &traced["cases"][0];
    assert_eq!(traced_case["status"], "passed");
    assert_eq!(traced_case["trace"]["dumps_requested"], true);
    let bundle = PathBuf::from(traced_case["bundle"].as_str().unwrap());
    for name in ["core.debug", "cc.debug", "mir.debug"] {
        assert!(bundle.join(name).is_file(), "missing retained {name}");
        assert!(
            traced_case["trace"]["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|artifact| artifact["retained_dump"] == name)
        );
    }
    assert!(value(&bundle.join("case.json"))["trace"].is_object());
    let trace_replay =
        std::fs::read_to_string(bundle.join("replay.sh")).expect("read trace replay script");
    assert!(trace_replay.contains("'diagnose'"));
    assert!(trace_replay.contains(" build "));

    let same_trace_compare = Command::new(env!("CARGO_BIN_EXE_psrs"))
        .args([
            "diagnose",
            "--compare",
            manifest.to_str().unwrap(),
            traced_manifest.to_str().unwrap(),
        ])
        .output()
        .expect("compare manifest and trace runs");
    assert!(same_trace_compare.status.success());
    let same_trace_report: serde_json::Value =
        serde_json::from_slice(&same_trace_compare.stdout).expect("parse trace comparison");
    assert_eq!(
        same_trace_report["changes"][0]["trace_comparison"]["status"],
        "no_observed_pass_difference"
    );
    assert_eq!(
        same_trace_report["changes"][0]["trace_comparison"]["artifact_content"],
        "unavailable_no_canonical_summary"
    );

    let mut changed = traced.clone();
    let passes = changed["cases"][0]["trace"]["executions"]
        .as_array_mut()
        .unwrap();
    passes[0]["status"] = "rejected".into();
    std::fs::write(
        &changed_manifest,
        serde_json::to_vec_pretty(&changed).unwrap(),
    )
    .expect("write changed trace fixture");
    let changed_compare = Command::new(env!("CARGO_BIN_EXE_psrs"))
        .args([
            "diagnose",
            "--compare",
            traced_manifest.to_str().unwrap(),
            changed_manifest.to_str().unwrap(),
        ])
        .output()
        .expect("compare pass status change");
    assert!(changed_compare.status.success());
    let changed_report: serde_json::Value =
        serde_json::from_slice(&changed_compare.stdout).expect("parse changed comparison");
    assert_eq!(
        changed_report["changes"][0]["trace_comparison"]["first_pass_difference"]["kind"],
        "pass_status_changed"
    );

    let mut legacy = before;
    legacy["schema_version"] = 1.into();
    legacy.as_object_mut().unwrap().remove("environment");
    legacy.as_object_mut().unwrap().remove("trace_mode");
    legacy["cases"][0].as_object_mut().unwrap().remove("trace");
    std::fs::write(
        &legacy_manifest,
        serde_json::to_vec_pretty(&legacy).unwrap(),
    )
    .expect("write v1 snapshot fixture");
    let comparison = Command::new(env!("CARGO_BIN_EXE_psrs"))
        .args([
            "diagnose",
            "--compare",
            legacy_manifest.to_str().unwrap(),
            traced_manifest.to_str().unwrap(),
        ])
        .output()
        .expect("compare v1 and v2 snapshots");
    assert!(
        comparison.status.success(),
        "v1 comparison failed: {}",
        String::from_utf8_lossy(&comparison.stderr)
    );
    let comparison_json: serde_json::Value =
        serde_json::from_slice(&comparison.stdout).expect("parse compare report");
    assert_eq!(
        comparison_json["observed_environment_compatibility"],
        "unavailable_legacy_snapshot"
    );
    assert_eq!(
        comparison_json["build_toolchain_compatibility"],
        "unavailable_not_embedded_in_compiler_binary"
    );
    assert!(comparison_json["changes"][0]["trace_comparison"].is_null());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn diagnose_input_list_is_ordered_exact_and_replayed_with_trace() {
    let root = test_dir("diagnose-inputs");
    let main = root.join("Main.purs");
    let spare = root.join("Spare.purs");
    let helper = root.join("Helper.purs");
    let hidden = root.join("Hidden.purs");
    let snapshot = root.join("snapshot.json");
    std::fs::write(
        &main,
        "module Main where\nimport Prelude\nimport Spare\nimport Helper\nmain = spare + helper\n",
    )
    .expect("write entry source");
    std::fs::write(&spare, "module Spare where\nspare = 1\n").expect("write first explicit input");
    std::fs::write(
        &helper,
        "module Helper where\nimport Hidden\nhelper = hidden\n",
    )
    .expect("write second explicit input");
    std::fs::write(&hidden, "module Hidden where\nhidden = 2\n")
        .expect("write unlisted adjacent module");

    let result = diagnose(&[
        main.to_str().unwrap(),
        "--input",
        spare.to_str().unwrap(),
        "--input",
        helper.to_str().unwrap(),
        "--trace",
        "--out",
        snapshot.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "diagnosis command failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report = value(&snapshot);
    let case = &report["cases"][0];
    assert_eq!(case["status"], "failed");
    let sources = case["trace"]["artifacts"][0]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["logical_name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        sources,
        [
            main.to_str().unwrap(),
            spare.to_str().unwrap(),
            helper.to_str().unwrap()
        ]
    );
    assert!(
        case["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diagnostic| diagnostic["message"].as_str().unwrap().contains("Hidden"))
    );

    let bundle = PathBuf::from(case["bundle"].as_str().unwrap());
    let replay = Command::new("sh")
        .arg(bundle.join("replay.sh"))
        .current_dir(&root)
        .output()
        .expect("run trace replay");
    assert!(
        !replay.status.success(),
        "the unresolved import should fail"
    );
    assert!(
        bundle.join("replay-trace.json").is_file(),
        "replay did not recapture a trace"
    );

    let duplicate = diagnose(&[
        main.to_str().unwrap(),
        "--input",
        helper.to_str().unwrap(),
        "--input",
        root.join(".").join("Helper.purs").to_str().unwrap(),
    ]);
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("duplicate diagnosis input"));

    let corpus_input = diagnose(&["--corpus", "passing", "--input", helper.to_str().unwrap()]);
    assert!(!corpus_input.status.success());

    let _ = std::fs::remove_dir_all(root);
}

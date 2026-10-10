use std::process::Command;

#[test]
fn builds_linked_sources_from_the_cli() {
    let root = std::env::temp_dir().join(format!("psrs-cli-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create CLI test directory");
    let helper = root.join("Helper.purs");
    let main = root.join("Main.purs");
    let output = root.join("main.wasm");
    std::fs::write(&helper, "module Helper where\nanswer :: Int\nanswer = 40\n")
        .expect("write helper source");
    std::fs::write(
        &main,
        "module Main where\nimport Prelude\nimport Helper\nmain = answer + 2\n",
    )
    .expect("write main source");

    let result = Command::new(env!("CARGO_BIN_EXE_psrs"))
        .args([
            "build",
            helper.to_str().expect("helper path is UTF-8"),
            main.to_str().expect("main path is UTF-8"),
            "-o",
            output.to_str().expect("output path is UTF-8"),
        ])
        .output()
        .expect("run psrs build");
    assert!(
        result.status.success(),
        "psrs build failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&output).expect("read generated component");
    assert_eq!(&bytes[..8], b"\0asm\r\0\x01\0");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn prints_source_attributed_redundancy_warnings() {
    let root = std::env::temp_dir().join(format!("psrs-warning-cli-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create CLI test directory");
    let main = root.join("Main.purs");
    let output = root.join("main.wasm");
    std::fs::write(
        &main,
        "module Main where\ndata Choice = First | Second\nchoose input = case input of\n  First -> 1\n  First -> 2\n  _ -> 3\nmain = choose First\n",
    )
    .expect("write source with a redundant alternative");

    let result = Command::new(env!("CARGO_BIN_EXE_psrs"))
        .args([
            "build",
            main.to_str().expect("main path is UTF-8"),
            "-o",
            output.to_str().expect("output path is UTF-8"),
        ])
        .output()
        .expect("run psrs build");
    assert!(
        result.status.success(),
        "psrs build failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("Main.purs"),
        "missing warning source: {stderr}"
    );
    assert!(stderr.contains("P8 closure conversion warning"), "{stderr}");
    assert!(stderr.contains("redundant case alternative"), "{stderr}");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn check_commands_print_typecheck_warnings() {
    let root = std::env::temp_dir().join(format!("psrs-custom-warning-cli-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create CLI test directory");
    let library = root.join("Lib.purs");
    let main = root.join("Main.purs");
    let single_source = root.join("Single.purs");
    std::fs::write(
        &library,
        "module Lib where\nimport Prim.TypeError (class Warn, Text)\nfoo :: Warn (Text \"from library\") => Int -> Int\nfoo value = value\n",
    )
    .expect("write library source");
    std::fs::write(
        &main,
        "module Main where\nimport Prim.TypeError (class Warn, Text)\nimport Lib (foo)\nmain :: Int\nmain = foo 42\n",
    )
    .expect("write main source");
    std::fs::write(
        &single_source,
        "module Main where\nimport Prim.TypeError (class Warn, Text)\nfoo :: Warn (Text \"single source\") => Int -> Int\nfoo value = value\nmain :: Int\nmain = foo 42\n",
    )
    .expect("write single source");

    let single = Command::new(env!("CARGO_BIN_EXE_psrs"))
        .args(["check", single_source.to_str().expect("UTF-8 source path")])
        .output()
        .expect("run psrs check");
    assert!(
        single.status.success(),
        "check rejected warnings: {single:?}"
    );
    let single_stderr = String::from_utf8_lossy(&single.stderr);
    assert!(
        single_stderr.contains("UserDefinedWarning"),
        "{single_stderr}"
    );
    assert!(single_stderr.contains("single source"), "{single_stderr}");

    let program = Command::new(env!("CARGO_BIN_EXE_psrs"))
        .args([
            "check-program",
            library.to_str().expect("UTF-8 library path"),
            main.to_str().expect("UTF-8 main path"),
        ])
        .output()
        .expect("run psrs check-program");
    assert!(
        program.status.success(),
        "check-program rejected warnings: {program:?}"
    );
    let program_stderr = String::from_utf8_lossy(&program.stderr);
    assert!(program_stderr.contains("Main.purs"), "{program_stderr}");
    assert!(
        program_stderr.contains("UserDefinedWarning"),
        "{program_stderr}"
    );
    assert!(program_stderr.contains("from library"), "{program_stderr}");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn diagnose_writes_a_diagnostic_bundle_that_replays_outside_the_bundle() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after UNIX epoch")
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("psrs-diagnose-cli-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create diagnosis test directory");
    let source = root.join("Broken.purs");
    let snapshot = root.join("snapshot.json");
    std::fs::write(&source, "module Broken where\nmain = @\n").expect("write malformed source");

    let result = Command::new(env!("CARGO_BIN_EXE_psrs"))
        .args([
            "diagnose",
            source.to_str().expect("source path is UTF-8"),
            "--out",
            snapshot.to_str().expect("snapshot path is UTF-8"),
        ])
        .output()
        .expect("run psrs diagnose");
    assert!(
        result.status.success(),
        "diagnosis command failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&snapshot).expect("read diagnosis snapshot"))
            .expect("parse diagnosis snapshot");
    let case = &report["cases"][0];
    assert_eq!(case["status"], "failed");
    assert_eq!(case["input_set_complete"], true);
    let diagnostic = &case["diagnostics"][0];
    assert!(!diagnostic["stage"].as_str().unwrap_or_default().is_empty());
    assert!(
        !diagnostic["message"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );

    let bundle = std::path::PathBuf::from(
        case["bundle"]
            .as_str()
            .expect("failed case has bundle path"),
    );
    assert!(bundle.join("case.json").is_file());
    assert!(bundle.join("inputs/0000-Broken.purs").is_file());
    let replay = Command::new("sh")
        .arg(bundle.join("replay.sh"))
        .current_dir(&root)
        .output()
        .expect("replay diagnosis bundle from outside its directory");
    assert!(
        !replay.status.success(),
        "broken source unexpectedly compiled"
    );
    let replay_output = format!(
        "{}{}",
        String::from_utf8_lossy(&replay.stdout),
        String::from_utf8_lossy(&replay.stderr)
    );
    assert!(
        replay_output.contains(diagnostic["message"].as_str().unwrap()),
        "replay did not reproduce the recorded diagnostic: {replay_output}"
    );

    let _ = std::fs::remove_dir_all(root);
}

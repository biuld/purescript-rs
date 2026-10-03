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

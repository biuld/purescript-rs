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
        "module Main where\nimport Helper\nmain = answer + 2\n",
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

use super::*;

#[test]
fn differential_library_foreign_declarations_against_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    let native = "module Native where\nforeign import intAdd :: Int -> Int\nforeign import operation :: Int -> Int -> Int\ninfixl 4 operation as %%\n";
    let main = "module Main where\nimport Native\nmain :: Int\nmain = intAdd (1 %% 2)\n";
    let sources = [("Native.purs", native), ("Main.purs", main)];
    psrs_driver::check_program(&sources).expect("source foreign declarations should type check");
    let directory = std::env::temp_dir().join(format!(
        "psrs-official-library-foreign-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, source) in sources {
        std::fs::write(directory.join(name), source).unwrap();
    }
    std::fs::write(
        directory.join("Native.js"),
        "export const intAdd = x => x;\nexport const operation = x => y => x + y;\n",
    )
    .unwrap();
    let output = Command::new("purs")
        .arg("compile")
        .arg(directory.join("Native.purs"))
        .arg(directory.join("Main.purs"))
        .arg("-o")
        .arg(directory.join("output"))
        .output()
        .expect("run the official compiler");
    let _ = std::fs::remove_dir_all(directory);
    assert!(output.status.success(), "{output:?}");
}

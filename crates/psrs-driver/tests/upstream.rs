use std::path::{Path, PathBuf};
use std::process::Command;

fn corpus_root() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PURESCRIPT_REPO") {
        let base = PathBuf::from(path).join("tests/purs");
        if base.is_dir() {
            return Some(base);
        }
        return None;
    }
    let vendored = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream");
    vendored.is_dir().then_some(vendored)
}

fn purs_available() -> bool {
    Command::new("purs")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn purs_accepts(source: &Path) -> bool {
    let output_dir = std::env::temp_dir().join(format!("psrs-purs-out-{}", std::process::id()));
    Command::new("purs")
        .arg("compile")
        .arg(source)
        .arg("-o")
        .arg(&output_dir)
        .output()
        .expect("failed to run purs")
        .status
        .success()
}

#[test]
fn differential_against_purs_on_selected_cases() {
    let Some(repo) = corpus_root() else {
        eprintln!("skipping: vendored corpus not found and PURESCRIPT_REPO is unset");
        return;
    };
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    let mut failures = Vec::new();

    let reject_cases = ["failing/InfiniteType.purs", "failing/IntOutOfRange.purs"];
    for relative in reject_cases {
        let path = repo.join(relative);
        if !path.is_file() {
            failures.push(format!("missing upstream case `{relative}`"));
            continue;
        }
        if purs_accepts(&path) {
            failures.push(format!("`{relative}`: purs accepted a failing test"));
        }
        let text = std::fs::read_to_string(&path).expect("read upstream test");
        if psrs_driver::check_source(&path.to_string_lossy(), &text).is_ok() {
            failures.push(format!("`{relative}`: psrs accepted but should reject"));
        }
    }

    let accept_cases: [(&str, &str); 2] = [
        (
            "identity.purs",
            "module Main where\nidentity :: forall a. a -> a\nidentity x = x\n",
        ),
        (
            "const.purs",
            "module Main where\nconst :: forall a b. a -> b -> a\nconst x y = x\n",
        ),
    ];
    for (name, source) in accept_cases {
        let path = std::env::temp_dir().join(format!("psrs-{}-{name}", std::process::id()));
        std::fs::write(&path, source).expect("write fixture");
        if !purs_accepts(&path) {
            failures.push(format!("`{name}`: purs rejected an accept case"));
        }
        if let Err(errors) = psrs_driver::check_source(name, source) {
            failures.push(format!("`{name}`: psrs rejected: {errors:?}"));
        }
        let _ = std::fs::remove_file(&path);
    }

    let chain_cases: [(&str, &str, bool); 6] = [
        (
            "instance-chain.purs",
            "module Main where\nclass Mark a where\n  mark :: a -> Int\ninstance markBoolean :: Mark Boolean where\n  mark _ = 1\nelse instance markFallback :: Mark a where\n  mark _ = 2\nmain :: Int\nmain = mark true\n",
            true,
        ),
        (
            "instance-chain-unknown.purs",
            "module Main where\nclass Same a b where\n  same :: a -> b -> Int\ninstance sameInt :: Same Int Int where\n  same _ _ = 1\nelse instance sameFallback :: Same a b where\n  same _ _ = 2\nuse :: forall a. a -> Int\nuse x = same x 0\nmain :: Int\nmain = use true\n",
            false,
        ),
        (
            "instance-chain-independent-fundep-argument.purs",
            "module Main where\nclass Select a b c | a -> b where\n  choose :: a -> c -> Int\ninstance first :: Select Int Boolean Int where\n  choose _ _ = 0\nelse instance fallback :: Select a Int c where\n  choose _ _ = 42\nmain :: Int\nmain = choose 1 true\n",
            true,
        ),
        (
            "instance-chain-transitive-fundep.purs",
            "module Main where\nclass Select a b c | a -> b, b -> c where\n  choose :: a -> Int\ninstance first :: Select Int Boolean Number where\n  choose _ = 42\nelse instance fallback :: Select Int String Char where\n  choose _ = 0\nmain :: Int\nmain = choose 1\n",
            true,
        ),
        (
            "instance-chain-repeated-head-occurs.purs",
            "module Main where\nclass Same a b where\n  same :: a -> b -> Int\ninstance repeated :: Same a a where\n  same _ _ = 0\nelse instance fallback :: Same a b where\n  same _ _ = 42\nuse :: forall a. a -> Array a -> Int\nuse x xs = same x xs\nmain :: Int\nmain = use 1 [1]\n",
            true,
        ),
        (
            "instance-chain-recursive-application-head.purs",
            "module Main where\nclass Arg i o | i -> o where\n  arg :: i -> Int\ninstance appArg :: Arg i o => Arg (f i) o where\n  arg _ = 42\nelse instance reflArg :: Arg a a where\n  arg _ = 0\nidentity :: Int -> Int\nidentity x = x\nmain :: Int\nmain = arg identity\n",
            true,
        ),
    ];
    for (name, source, expected_acceptance) in chain_cases {
        let path = std::env::temp_dir().join(format!("psrs-{}-{name}", std::process::id()));
        std::fs::write(&path, source).expect("write instance-chain fixture");
        let purs_accepted = purs_accepts(&path);
        let psrs_accepted = psrs_driver::check_source(name, source).is_ok();
        if purs_accepted != expected_acceptance {
            failures.push(format!(
                "`{name}`: purs accepted={purs_accepted}, expected={expected_acceptance}"
            ));
        }
        if psrs_accepted != expected_acceptance {
            failures.push(format!(
                "`{name}`: psrs accepted={psrs_accepted}, expected={expected_acceptance}"
            ));
        }
        let _ = std::fs::remove_file(&path);
    }

    assert!(
        failures.is_empty(),
        "upstream differential failures:\n{}",
        failures.join("\n")
    );
}

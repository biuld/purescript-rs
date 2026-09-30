use std::path::{Path, PathBuf};
use std::process::Command;

type SourceFile = (&'static str, &'static str);
type SourceSet<'a> = &'a [SourceFile];
type CoercionDifferentialCase<'a> = (&'a str, SourceSet<'a>, SourceSet<'a>, bool);

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

fn purs_accepts_sources(name: &str, sources: &[SourceFile]) -> bool {
    let case_dir =
        std::env::temp_dir().join(format!("psrs-purs-upstream-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&case_dir);
    std::fs::create_dir_all(&case_dir).expect("create upstream fixture directory");
    let paths = sources
        .iter()
        .map(|(name, source)| {
            let path = case_dir.join(name);
            std::fs::write(&path, source).expect("write upstream fixture");
            if *name == "Safe.Coerce.purs" {
                std::fs::write(
                    case_dir.join("Safe.Coerce.js"),
                    "export const unsafeCoerce = value => value;\n",
                )
                .expect("write minimal coercion FFI");
            }
            path
        })
        .collect::<Vec<_>>();
    let output_dir = case_dir.join("output");
    let accepted = Command::new("purs")
        .arg("compile")
        .args(&paths)
        .arg("-o")
        .arg(output_dir)
        .output()
        .expect("failed to run purs")
        .status
        .success();
    let _ = std::fs::remove_dir_all(case_dir);
    accepted
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

#[test]
fn differential_role_and_coercible_rules_against_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    let coercion_module = "module Safe.Coerce where\n\
        import Prim.Coerce (class Coercible)\n\
        foreign import unsafeCoerce :: forall a b. a -> b\n\
        coerce :: forall a b. Coercible a b => a -> b\n\
        coerce = unsafeCoerce\n";

    let local_nominal = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        data Box a = Box a\n\
        type role Box nominal\n\
        newtype Age = Age Int\n\
        bad :: Box Age -> Box Int\n\
        bad = coerce\n";
    let imported_nominal_lib = "module Lib (Box(..)) where\n\
        data Box a = Box a\n\
        type role Box nominal\n";
    let imported_nominal_main = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        import Lib (Box(..))\n\
        newtype Age = Age Int\n\
        bad :: Box Age -> Box Int\n\
        bad = coerce\n";
    let representational_data = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        data Box a = Box a\n\
        newtype Age = Age Int\n\
        unbox (Box value) = value\n\
        main :: Int\n\
        main = unbox (coerce (Box (Age 109)))\n";
    let phantom_sum = r#"module Main where
import Safe.Coerce (coerce)
data Phantom a = Empty | Marked Int
convert :: Phantom Int -> Phantom Boolean
convert = coerce
main :: Int
main = case convert (Marked 42) of
  Empty -> 0
  Marked value -> value
"#;
    let nominal_newtype = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Box a = Box a\n\
        type role Box nominal\n\
        newtype Age = Age Int\n\
        unbox (Box value) = value\n\
        main :: Int\n\
        main = unbox (coerce (Box (Age 127)))\n";
    let hidden_lib = "module HiddenLib (Age, age) where\n\
        newtype Age = Age Int\n\
        age :: Int -> Age\n\
        age = Age\n";
    let hidden_main = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        import HiddenLib (Age, age)\n\
        main :: Int\n\
        main = coerce (age 42)\n";
    let weakened_role = "module Main where\n\
        data Box a = Box a\n\
        type role Box phantom\n\
        main :: Int\n\
        main = 0\n";
    let alias_lib = "module BoxLib (Alias, Box(..)) where\n\
        type Alias a = a\n\
        data Box a = Box (Alias a)\n";
    let alias_main = "module Main where\n\
        import BoxLib (Alias, Box(..))\n\
        import Safe.Coerce (coerce)\n\
        newtype Age = Age Int\n\
        unbox (Box value) = value\n\
        main :: Int\n\
        main = unbox (coerce (Box (Age 137)))\n";
    let transitive_official = "module Main where\n\
        import Prim.Coerce (class Coercible)\n\
        import Safe.Coerce (coerce)\n\
        newtype Age = Age Int\n\
        newtype Raw = Raw Int\n\
        convert :: forall a b c. Coercible a b => Coercible b c => a -> b -> c\n\
        convert value _ = coerce value\n\
        main :: Int\n\
        main = convert (Age 131) (Raw 0)\n";
    let transitive_compiler = "module Main where\n\
        import Safe.Coerce (class Coercible, coerce)\n\
        newtype Age = Age Int\n\
        newtype Raw = Raw Int\n\
        convert :: forall a b c. Coercible a b => Coercible b c => a -> b -> c\n\
        convert value _ = coerce value\n\
        main :: Int\n\
        main = convert (Age 131) (Raw 0)\n";
    let parameterized_scalar = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Box a = Box a\n\
        main :: Int\n\
        main = coerce (Box 89)\n";
    let parameterized_function = r#"module Main where
import Safe.Coerce (coerce)
newtype Endo a = Endo (a -> a)
coerceEndo :: Endo Int -> Int -> Int
coerceEndo = coerce
main :: Int
main = coerceEndo (Endo (\value -> value)) 37
"#;
    let parameterized_array = "module Main where\n\
        import Safe.Coerce (coerce)\n\
        newtype Items a = Items (Array a)\n\
        unwrap :: Items Int -> Array Int\n\
        unwrap = coerce\n\
        main :: Int\n\
        main = 0\n";

    let cases: [CoercionDifferentialCase<'_>; 12] = [
        (
            "role-local-nominal",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", local_nominal),
            ],
            &[("Main.purs", local_nominal)],
            false,
        ),
        (
            "role-imported-nominal",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Lib.purs", imported_nominal_lib),
                ("Main.purs", imported_nominal_main),
            ],
            &[
                ("Lib.purs", imported_nominal_lib),
                ("Main.purs", imported_nominal_main),
            ],
            false,
        ),
        (
            "coercible-representational-data",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", representational_data),
            ],
            &[("Main.purs", representational_data)],
            true,
        ),
        (
            "coercible-phantom-sum",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", phantom_sum),
            ],
            &[("Main.purs", phantom_sum)],
            true,
        ),
        (
            "coercible-visible-nominal-newtype",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", nominal_newtype),
            ],
            &[("Main.purs", nominal_newtype)],
            true,
        ),
        (
            "coercible-hidden-newtype",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("HiddenLib.purs", hidden_lib),
                ("Main.purs", hidden_main),
            ],
            &[("HiddenLib.purs", hidden_lib), ("Main.purs", hidden_main)],
            false,
        ),
        (
            "role-weakening",
            &[("Main.purs", weakened_role)],
            &[("Main.purs", weakened_role)],
            false,
        ),
        (
            "coercible-imported-alias-data",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("BoxLib.purs", alias_lib),
                ("Main.purs", alias_main),
            ],
            &[("BoxLib.purs", alias_lib), ("Main.purs", alias_main)],
            true,
        ),
        (
            "coercible-transitive-givens",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", transitive_official),
            ],
            &[("Main.purs", transitive_compiler)],
            true,
        ),
        (
            "coercible-parameterized-newtype-scalar",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", parameterized_scalar),
            ],
            &[("Main.purs", parameterized_scalar)],
            true,
        ),
        (
            "coercible-parameterized-newtype-function",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", parameterized_function),
            ],
            &[("Main.purs", parameterized_function)],
            true,
        ),
        (
            "coercible-parameterized-newtype-array",
            &[
                ("Safe.Coerce.purs", coercion_module),
                ("Main.purs", parameterized_array),
            ],
            &[("Main.purs", parameterized_array)],
            true,
        ),
    ];

    let mut failures = Vec::new();
    for (name, official_sources, compiler_sources, expected_acceptance) in cases {
        let purs_accepted = purs_accepts_sources(name, official_sources);
        let psrs_result = psrs_driver::check_program(compiler_sources);
        let psrs_accepted = psrs_result.is_ok();
        if purs_accepted != expected_acceptance {
            failures.push(format!(
                "`{name}`: purs accepted={purs_accepted}, expected={expected_acceptance}"
            ));
        }
        if psrs_accepted != expected_acceptance {
            failures.push(format!(
                "`{name}`: psrs accepted={psrs_accepted}, expected={expected_acceptance}; errors={:?}",
                psrs_result.err()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "role/Coercible differential failures:\n{}",
        failures.join("\n")
    );
}

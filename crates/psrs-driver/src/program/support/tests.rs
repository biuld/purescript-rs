use super::*;

const FUNCTOR: &str = r#"module Data.Functor where
class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b
"#;
const PROFUNCTOR: &str = r#"module Data.Profunctor where
class Profunctor p where
  dimap :: forall a b c d. (b -> a) -> (c -> d) -> p a c -> p b d
  lcmap :: forall a b c. (b -> a) -> p a c -> p b c
  rmap :: forall a b c. (b -> c) -> p a b -> p a c
instance functionProfunctor :: Profunctor (->) where
  dimap f g h = \x -> g (h (f x))
  lcmap f h = \x -> h (f x)
  rmap f h = \x -> f (h x)
hidden = 1
"#;
const MAIN: &str = r#"module Main where
import Data.Functor (class Functor, map)
data Twice a = Twice ((a -> Int) -> Int)
derive instance Functor Twice
main :: Int
main = case map (\_ -> 42) (Twice (\f -> f 0)) of
  Twice run -> run (\x -> x)
"#;

fn sources(main: &str) -> [(&str, &str); 3] {
    [
        ("Functor.purs", FUNCTOR),
        ("Profunctor.purs", PROFUNCTOR),
        ("Main.purs", main),
    ]
}

fn check(sources: &[(&str, &str)], prefix: usize) -> Result<(), Vec<ProgramDiagnostic>> {
    let modules = resolve_program_sources(sources)?;
    typecheck_resolved_program_with_warnings(modules, SupportEnvironment::Complete { prefix })
        .map(|_| ())
}

#[test]
fn complete_support_supplies_mapping_instances_without_importing_names() {
    let sources = sources(MAIN);
    let errors = crate::check_program(&sources).expect_err("ordinary import closure");
    assert!(
        errors
            .iter()
            .any(|e| e.diagnostic.code == Some("CannotDeriveInvalidConstructorArg")),
        "{errors:?}"
    );
    check(&sources, 2).expect("checked support supplies the function Profunctor instance");
    let hidden = "module Main where\nmain = hidden\n";
    let errors = check(&self::sources(hidden), 2).expect_err("names still need imports");
    assert!(
        errors
            .iter()
            .any(|e| e.diagnostic.code == Some("UnknownName")),
        "{errors:?}"
    );
}

#[test]
fn failed_support_producer_cannot_supply_a_successful_environment() {
    let sources = [
        ("Functor.purs", FUNCTOR),
        ("Profunctor.purs", PROFUNCTOR),
        (
            "Broken.purs",
            "module Broken where\nbad :: Int\nbad = true\n",
        ),
        ("Main.purs", MAIN),
    ];
    let errors = check(&sources, 3).expect_err("all support must succeed");
    assert!(
        errors
            .iter()
            .all(|e| e.source == DiagnosticOrigin::Source(2)),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.diagnostic.code == Some("TypesDoNotUnify")),
        "{errors:?}"
    );
}

#[test]
fn complete_support_does_not_authorize_a_same_named_user_class() {
    let sources = [
        ("Profunctor.purs", PROFUNCTOR),
        (
            "User.purs",
            "module User where\nclass Functor f where\n  map :: forall a b. (a -> b) -> f a -> f b\n",
        ),
        (
            "Main.purs",
            "module Main where\nimport User (class Functor)\ndata Box a = Box a\nderive instance Functor Box\nmain = 0\n",
        ),
    ];
    let errors = check(&sources, 2).expect_err("unrecognized class identity");
    assert!(
        errors
            .iter()
            .any(|e| e.diagnostic.code == Some("CannotDerive")),
        "{errors:?}"
    );
}

#[test]
fn invalid_support_inventory_is_an_explicit_error() {
    let errors = check(&sources(MAIN), 4).expect_err("invalid prefix");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].source, DiagnosticOrigin::Program);
    assert!(
        errors[0]
            .diagnostic
            .message
            .contains("support prefix exceeds")
    );
}

#[test]
fn unimported_support_mapping_dictionary_executes_the_derived_adapter() {
    use std::process::Command;
    if !Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        assert_ne!(std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(), Ok("1"));
        return;
    }
    let report = super::super::compilation::compile_attempt(
        &sources(MAIN),
        2,
        false,
        false,
        None,
        psrs_backend::TargetCapabilities::default(),
        SupportEnvironment::Complete { prefix: 2 },
    );
    let artifact = report
        .artifact
        .unwrap_or_else(|| panic!("{:?}", report.diagnostics));
    let path =
        std::env::temp_dir().join(format!("psrs-complete-support-{}.wasm", std::process::id()));
    std::fs::write(&path, artifact.wasm).unwrap();
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn reexported_deriving_class_keeps_its_canonical_support_identity() {
    let main = MAIN.replace("import Data.Functor", "import Umbrella");
    let sources = [
        ("Functor.purs", FUNCTOR),
        ("Profunctor.purs", PROFUNCTOR),
        (
            "Umbrella.purs",
            "module Umbrella (module F) where\nimport Data.Functor as F\n",
        ),
        ("Main.purs", main.as_str()),
    ];
    check(&sources, 3).expect("the class defining identity survives its reexport");
}

#[test]
fn complete_support_still_rejects_an_orphan_dictionary() {
    let sources = [
        (
            "Class.purs",
            "module Class where\nclass C a where\n  method :: a -> Int\n",
        ),
        ("Type.purs", "module Type where\ndata T = T\n"),
        (
            "Orphan.purs",
            "module Orphan where\nimport Class (class C)\nimport Type (T)\ninstance C T where\n  method _ = 42\n",
        ),
        ("Main.purs", "module Main where\nmain = 0\n"),
    ];
    let errors = check(&sources, 3).expect_err("support scope does not admit orphan instances");
    assert!(
        errors
            .iter()
            .any(|e| e.diagnostic.code == Some("OrphanInstance")),
        "{errors:?}"
    );
}

//! Region isolation and constructor-hiding on the official ST facade.

fn rejects(main: &str, code: &str) {
    let errors = crate::check_program_types_lenient_with_prelude(&[("Main.purs", main)])
        .expect_err("program should be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some(code)),
        "expected {code}, got {errors:?}"
    );
}

#[test]
fn run_rejects_an_escaping_st_reference() {
    rejects(
        r#"module Main where
import Prelude
import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect)

test _ = do
  r <- pure (ST.run (STRef.new 0))
  pure r

main :: forall a. a -> Effect Int
main = test
"#,
        "EscapedSkolem",
    );
}

#[test]
fn run_rejects_an_st_action_at_a_scalar_result() {
    rejects(
        r#"module Main where
import Prelude
import Control.Monad.ST (ST)
import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef

leak :: forall r. ST r (ST r Int)
leak = do
  ref <- STRef.new 0
  pure (STRef.read ref)

main :: Int
main = ST.run leak
"#,
        "TypesDoNotUnify",
    );
}

#[test]
fn run_rejects_an_inferred_escaping_st_action() {
    rejects(
        r#"module Main where
import Prelude
import Control.Monad.ST (ST)
import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef

leak :: forall r. ST r (ST r Int)
leak = do
  ref <- STRef.new 0
  pure (STRef.read ref)

escaped = ST.run leak

main :: Int
main = 0
"#,
        "EscapedSkolem",
    );
}

#[test]
fn a_reference_cannot_be_read_in_another_region() {
    rejects(
        r#"module Main where
import Control.Monad.ST (ST)
import Control.Monad.ST.Ref (STRef, read)

bad :: forall r s a. STRef r a -> ST s a
bad = read

main :: Int
main = 0
"#,
        "TypesDoNotUnify",
    );
}

#[test]
fn nominal_stref_region_rejects_coerce() {
    rejects(
        r#"module Main where
import Control.Monad.ST.Ref (STRef)
import Safe.Coerce (coerce)

bad :: forall r s a. STRef r a -> STRef s a
bad = coerce

main :: Int
main = 0
"#,
        "NoInstanceFound",
    );
}

#[test]
fn host_clock_is_not_an_st_request() {
    rejects(
        r#"module Main where
import Control.Monad.ST (ST)
import WASI.Clock (now)

bad :: forall r. ST r Int
bad = now

main :: Int
main = 0
"#,
        "TypesDoNotUnify",
    );
}

#[test]
fn the_st_constructor_is_not_exported() {
    rejects(
        r#"module Main where
import Control.Monad.ST (ST)

unwrap :: forall r a. ST r a -> Int
unwrap (ST _) = 0

main :: Int
main = 0
"#,
        "UnknownName",
    );
}

#[test]
fn st_and_stref_regions_keep_the_official_region_kind() {
    for name in ["ST", "STRef"] {
        let source = format!(
            "module Main where\nimport Control.Monad.ST.Internal ({name})\nforeign import bad :: {name} Int Int\nmain :: Int\nmain = 0\n"
        );
        let errors = crate::check_program_kinds_lenient_with_prelude(&[("Main.purs", &source)])
            .expect_err("a Type cannot inhabit the Region kind");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.code == Some("KindsDoNotUnify")),
            "{errors:?}"
        );
    }
}

#[test]
fn public_array_and_uncurried_adapters_keep_foreign_constructors_hidden() {
    for (module, constructor) in [
        ("Data.Array.ST", "STArray"),
        ("Control.Monad.ST.Uncurried", "STFn1"),
    ] {
        let source = format!(
            "module Main where\nimport {module} as A\nunwrap (A.{constructor} value) = value\nmain :: Int\nmain = 0\n"
        );
        rejects(&source, "UnknownName");
    }
}

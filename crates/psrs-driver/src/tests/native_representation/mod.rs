//! Official Effect/ST adapters and generic representation regressions.

mod algebraic;
mod arrays;
mod command;
mod host;
mod identity;
mod region;
mod semantics;
mod sources;
mod uncurried;

use sources::ENTRY;

const NAIVE_NOMINAL_EFFECT: &str = r#"module Main where
data Native f a = Return a | Suspend (f (Native f a))
data NativeF next = NativeF next
newtype Effect a = Effect (Native NativeF a)
type role Effect representational
main :: Int
main = 0
"#;

const PHANTOM_FREE_ROLES: &str = r#"module Main where
data Val
data View f = Ret Val | Bound (f Val) (Val -> Native f Val)
data Native f a = Native (View f)
data NativeF next = NativeF next
data Global
newtype Effect a = Effect (Native NativeF a)
type role Effect representational
newtype ST r a = ST (Native NativeF a)
type role ST nominal representational
newtype STRef r a = STRef Int
type role STRef nominal representational
main :: Int
main = 0
"#;

fn rejects_role(source: &str) {
    let errors = crate::check_program(&[("Main.purs", source)]).expect_err("should be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("RoleMismatch")),
        "expected RoleMismatch, got {errors:?}"
    );
}

#[test]
fn a_naive_free_monad_rejects_a_representational_effect_role() {
    rejects_role(NAIVE_NOMINAL_EFFECT);
}

#[test]
fn a_phantom_free_native_program_accepts_official_effect_and_st_roles() {
    crate::check_program(&[("Main.purs", PHANTOM_FREE_ROLES)]).unwrap_or_else(|errors| {
        panic!("phantom Free should accept official Effect/ST roles: {errors:?}")
    });
}

#[test]
fn official_st_and_effect_run_storage_and_fresh_refs() {
    let Some(output) = super::run_program_with_prelude_wasmtime(&[("Main.purs", ENTRY)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

const HOST_CLOCK: &str = r#"module Main where
import Prelude
import Effect.Unsafe (unsafePerformEffect)
import PSRS.Effect.Host as Host
foreign import "wasi:clocks/monotonic-clock#now" clockNow :: Int
main :: Int
main = unsafePerformEffect (Host.lift \_ -> clockNow) * 0 + 42
"#;

#[test]
fn host_lift_delays_a_raw_canonical_clock_import() {
    let Some(output) = super::run_program_with_prelude_wasmtime(&[("Main.purs", HOST_CLOCK)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

const OFFICIAL_CLOCK: &str = r#"module Main where
import Prelude
import Effect.Unsafe (unsafePerformEffect)
import WASI.Clock (now)
main :: Int
main = unsafePerformEffect now * 0 + 42
"#;

#[test]
fn official_clock_now_runs_through_effect() {
    let Some(output) = super::run_program_with_prelude_wasmtime(&[("Main.purs", OFFICIAL_CLOCK)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

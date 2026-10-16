//! Host-resource capture and instance-terminal trap boundaries.
use super::SOURCE;

const HEADER: &str = r"module Main where
import Prim.Control as C
import Unsafe.Coerce (unsafeCoerce)
foreign import data File :: Type
data Op x = Op (Int -> x)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler state (Op k) raw = { _1: state, _2: raw (k 42) }
";

#[test]
fn cc_control_callback_cannot_capture_a_host_resource_handle() {
    let source = format!(
        "{HEADER}\nmain :: Int\nmain = let file = unsafeCoerce 0 :: File\n           owner = C.once (\\n -> let _ = file in C.return n)\n       in (C.run 0 handler (C.resume owner 1))._2\n"
    );
    let errors = crate::lower_main_to_cc(&source).expect_err("a File capture must fail P8");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("control callback cannot capture a host resource handle")),
        "{errors:?}"
    );
}

#[test]
fn cc_control_callback_may_capture_an_ordinary_integer() {
    let source = format!(
        "{HEADER}\nmain :: Int\nmain = let n = 1\n           owner = C.once (\\x -> C.return (intAdd x n))\n       in (C.run 0 handler (C.resume owner 1))._2\n"
    );
    crate::lower_main_to_cc(&source).expect("an Int capture is not a host resource");
}

#[test]
fn control_artifact_has_no_post_trap_cleanup_export() {
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let artifact = psrs_backend::compile_cc(input).unwrap();
    for forbidden in [
        "control_cleanup",
        "control_recover",
        "psrs_control_cleanup",
        "cabi_post_control",
    ] {
        assert!(
            !artifact.wat.contains(forbidden),
            "instance-terminal traps must not reenter through {forbidden}"
        );
    }
    assert!(
        artifact.wat.contains("wasi:cli/run@0.2.12#run") || artifact.wat.contains("wasi:cli/run"),
        "the artifact still exports the ordinary run contract"
    );
}

#[test]
fn a_trap_during_control_is_instance_terminal() {
    let source =
        format!("{HEADER}\nmain :: Int\nmain = (C.run 0 handler (C.return (intQuot 1 0)))._2\n");
    let Some(output) = super::super::run_with_wasmtime(&source) else {
        return;
    };
    assert!(
        !output.status.success(),
        "a trap must abort the instance: {output:?}"
    );
}

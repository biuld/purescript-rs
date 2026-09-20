use super::*;

#[test]
fn resolves_stdout_and_exit_imports() {
    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let stdout = registry
        .import(names::STDOUT, names::GET_STDOUT)
        .expect("get-stdout should resolve");
    assert_eq!(stdout.module, "wasi:cli/stdout@0.2.12");
    assert!(stdout.parameters.is_empty());
    assert!(stdout.param_kinds.is_empty());
    assert_eq!(stdout.result, Some(ValueType::I32));

    let write = registry
        .import(names::STREAMS, names::WRITE_STDOUT)
        .expect("blocking-write-and-flush should resolve");
    assert_eq!(write.module, "wasi:io/streams@0.2.12");
    assert_eq!(
        write.param_kinds,
        vec![WasiParamKind::Handle, WasiParamKind::List]
    );
    assert_eq!(write.result_kind, WasiResultKind::Result);
    assert!(write.retptr);

    let exit = registry
        .import(names::EXIT, names::EXIT_WITH_CODE)
        .expect("exit-with-code should resolve");
    assert_eq!(exit.module, "wasi:cli/exit@0.2.12");
    assert_eq!(exit.parameters, vec![ValueType::I32]);
    assert_eq!(exit.result, None);

    // Interning returns the same symbol for the same import.
    let stdout_again = registry
        .import(names::STDOUT, names::GET_STDOUT)
        .expect("get-stdout should resolve again");
    assert_eq!(stdout.symbol, stdout_again.symbol);
}

#[test]
fn classifies_a_64_bit_parameter_by_its_wit_signedness() {
    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let random = registry
        .import("wasi:random/random", "get-random-bytes")
        .expect("get-random-bytes should resolve");
    assert_eq!(
        random.param_kinds,
        vec![WasiParamKind::Scalar64 { signed: false }]
    );
}

#[test]
fn rejects_a_disabled_wasi_service_before_lowering() {
    let target = TargetCapabilities {
        wasi_random: false,
        ..TargetCapabilities::default()
    };
    let mut registry = WasiRegistry::load_with_capabilities(target)
        .expect("WASI WIT should load with a restricted target");
    let random = registry
        .import("wasi:random/random", "get-random-bytes")
        .expect("the WIT declaration should still resolve");
    assert!(
        random
            .unsupported
            .as_deref()
            .is_some_and(|message| message.contains("disabled"))
    );
}

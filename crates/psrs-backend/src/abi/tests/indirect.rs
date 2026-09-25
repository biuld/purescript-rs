use super::*;
use wit_parser::abi::{AbiVariant, WasmType};

#[test]
fn accepts_the_canonical_indirect_parameter_signature() {
    let wit = "package wasi:io@0.2.12; interface streams { take: func(a0: s32, a1: s32, a2: s32, a3: s32, a4: s32, a5: s32, a6: s32, a7: s32, a8: s32, a9: s32, a10: s32, a11: s32, a12: s32, a13: s32, amount: f64, enabled: bool, wide: u64, ratio: f32); }";
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str("indirect-params.wit", wit)
        .expect("the indirect WIT fixture should resolve");
    let interface = resolve.packages[package].interfaces["streams"];
    let function = &resolve.interfaces[interface].functions["take"];
    let canonical = resolve.wasm_signature(AbiVariant::GuestImport, function);
    assert!(canonical.indirect_params);
    assert_eq!(canonical.params, vec![WasmType::Pointer]);

    let mut registry = WasiRegistry::from_resolve(resolve, TargetCapabilities::default());
    let import = registry
        .import("wasi:io/streams", "take")
        .expect("the WIT function should resolve through the ABI registry");
    assert!(import.unsupported.is_none());
    assert!(import.has_indirect_parameters());
    assert_eq!(import.parameters, vec![ValueType::I32]);
}

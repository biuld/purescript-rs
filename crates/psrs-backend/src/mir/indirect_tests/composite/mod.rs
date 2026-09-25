mod assertions;
mod fixture;

use super::lower_module_with_registry;
use crate::TargetCapabilities;
use crate::abi;

#[test]
fn indirect_composite_parameters_lower_to_the_canonical_layout() {
    let (cc, bindings, resolve) = fixture::composite_indirect_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower composite indirect canonical parameters");
    let import = registry
        .import("wasi:io/streams", "take-shapes")
        .expect("the composite WIT function should resolve");
    assert_eq!(import.parameters, vec![crate::types::ValueType::I32]);
    assertions::assert_wit_shape(&import);
    assertions::assert_p9_layout(&mir, &import);

    let mir =
        crate::mir::opt::optimize(mir, target).expect("P10 should preserve composite ABI stores");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the composite indirect call and allocator");
    let binary =
        crate::wasm::encode_module(&wasm).expect("the composite Wasm artifact should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the composite Wasm artifact should validate");
    assertions::assert_wasm_artifact(&wasm, &import, &binary);
}

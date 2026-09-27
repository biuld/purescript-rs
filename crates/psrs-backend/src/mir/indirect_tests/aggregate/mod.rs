//! Synthesized `result`/`option`/`variant` aggregate tests, including nested
//! record and variant payloads.

mod fixtures;
mod nested;
mod scalars;

use super::lower_module_with_registry;
use crate::ExternalBindings;
use crate::TargetCapabilities;
use crate::abi;
use crate::cc::{self, VariantCase};
use fixtures::{erased, fixture, parameter_fixture};
use nested::{nested_record_fixture, nested_record_parameter_fixture, nested_variant_fixture};
use scalars::{wide_scalar_fixture, wide_scalar_parameter_fixture};
use wit_parser::Resolve;

fn lower_and_validate(module: cc::Module, bindings: ExternalBindings, resolve: Resolve) {
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(module, bindings, target, registry)
        .expect("P9 should lower the aggregate result");

    let has_switch = mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                Some(crate::mir::Terminator::Switch { .. })
            )
        })
    });
    assert!(
        has_switch,
        "the result discriminant should dispatch on a switch"
    );
    let has_variant_new = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .any(|instruction| matches!(instruction, crate::mir::Instruction::StructNew { .. }));
    assert!(has_variant_new, "the result should build a variant case");

    let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the result ABI");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the aggregate result");
    let binary = crate::wasm::encode_module(&wasm).expect("the aggregate Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the aggregate Wasm should validate");
}

/// `option<s32>` -> `Data.Maybe.Maybe Int` passed as a parameter. The main
/// function builds `Just 42` by boxing the integer into the erased field.
#[test]
fn aggregate_parameter_lowers_to_a_wasm_artifact() {
    let (module, bindings, resolve) = parameter_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(module, bindings, target, registry)
        .expect("P9 should lower an aggregate parameter");

    let has_switch = mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                Some(crate::mir::Terminator::Switch { .. })
            )
        })
    });
    assert!(
        has_switch,
        "the parameter discriminant should dispatch on a switch"
    );

    let mir =
        crate::mir::opt::optimize(mir, target).expect("P10 should preserve the parameter ABI");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the aggregate parameter");
    let binary = crate::wasm::encode_module(&wasm).expect("the aggregate Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the aggregate parameter Wasm should validate");
}

#[test]
fn result_aggregate_lowers_to_a_wasm_artifact() {
    // result<list<u8>, s32> -> Either String Int
    let (module, bindings, resolve) = fixture(
        "package wasi:io@0.2.12; interface streams { get: func() -> result<list<u8>, s32>; }",
        "get",
        vec![
            VariantCase {
                tag: 0,
                fields: vec![erased()],
            },
            VariantCase {
                tag: 1,
                fields: vec![erased()],
            },
        ],
    );
    lower_and_validate(module, bindings, resolve);
}

/// `result<list<u8>, err>` -> `Either String Err` where `err` is itself a WIT
/// variant, so the error payload is a nested source variant.
#[test]
fn nested_variant_payload_lowers_to_a_wasm_artifact() {
    let (module, bindings, resolve) = nested_variant_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(module, bindings, target, registry)
        .expect("P9 should lower a nested variant payload");
    let switches = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .filter(|block| {
            matches!(
                block.terminator,
                Some(crate::mir::Terminator::Switch { .. })
            )
        })
        .count();
    assert!(
        switches >= 2,
        "the outer and nested tags both dispatch on a switch"
    );

    let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the nested ABI");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the nested variant payload");
    let binary = crate::wasm::encode_module(&wasm).expect("the nested Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the nested variant Wasm should validate");
}

/// `option<pair>` -> `Maybe Pair` where `pair` is a WIT record, so the payload
/// is a nested source record.
#[test]
fn nested_record_payload_lowers_to_a_wasm_artifact() {
    let (module, bindings, resolve) = nested_record_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(module, bindings, target, registry)
        .expect("P9 should lower a nested record payload");
    let struct_new = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| matches!(instruction, crate::mir::Instruction::StructNew { .. }))
        .count();
    assert!(
        struct_new >= 2,
        "the record and the variant case both build a struct"
    );

    let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the nested ABI");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the nested record payload");
    let binary = crate::wasm::encode_module(&wasm).expect("the nested Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the nested record Wasm should validate");
}

#[test]
fn nested_record_parameter_lowers_to_a_wasm_artifact() {
    let (module, bindings, resolve) = nested_record_parameter_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(module, bindings, target, registry)
        .expect("P9 should lower a nested record parameter");
    let switches = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .filter(|block| {
            matches!(
                block.terminator,
                Some(crate::mir::Terminator::Switch { .. })
            )
        })
        .count();
    assert_eq!(switches, 1, "the Maybe tag dispatches on one switch");

    let mir =
        crate::mir::opt::optimize(mir, target).expect("P10 should preserve the parameter ABI");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the nested record parameter");
    let binary = crate::wasm::encode_module(&wasm).expect("the nested Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the nested record parameter Wasm should validate");
}

#[test]
fn wide_scalar_payloads_lower_to_a_wasm_artifact() {
    let (module, bindings, resolve) = wide_scalar_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(module, bindings, target, registry)
        .expect("P9 should lower wide scalar payloads");
    let loads = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            matches!(
                instruction,
                crate::mir::Instruction::LoadI64 { .. } | crate::mir::Instruction::LoadF64 { .. }
            )
        })
        .count();
    assert_eq!(loads, 2, "both wide payloads are read at their width");

    let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the wide ABI");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower wide scalar payloads");
    let binary = crate::wasm::encode_module(&wasm).expect("the wide Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the wide scalar Wasm should validate");
}

#[test]
fn wide_scalar_parameter_lowers_to_a_wasm_artifact() {
    let (module, bindings, resolve) = wide_scalar_parameter_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(module, bindings, target, registry)
        .expect("P9 should lower a wide scalar parameter");
    let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the wide ABI");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the wide scalar parameter");
    let binary = crate::wasm::encode_module(&wasm).expect("the wide Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the wide scalar parameter Wasm should validate");
}

#[test]
fn result_with_an_enum_payload_reads_a_narrow_discriminant() {
    // result<list<u8>, error-code> -> Either String ErrorCode
    let (module, bindings, resolve) = fixture(
        "package wasi:io@0.2.12; interface streams { enum error-code { a, b, c } get: func() -> result<list<u8>, error-code>; }",
        "get",
        vec![
            VariantCase {
                tag: 0,
                fields: vec![erased()],
            },
            VariantCase {
                tag: 1,
                fields: vec![erased()],
            },
        ],
    );
    lower_and_validate(module, bindings, resolve);
}

#[test]
fn option_aggregate_lowers_to_a_wasm_artifact() {
    // option<string> -> Maybe String
    let (module, bindings, resolve) = fixture(
        "package wasi:io@0.2.12; interface streams { get: func() -> option<string>; }",
        "get",
        vec![
            VariantCase {
                tag: 0,
                fields: Vec::new(),
            },
            VariantCase {
                tag: 1,
                fields: vec![erased()],
            },
        ],
    );
    lower_and_validate(module, bindings, resolve);
}

#[test]
fn variant_aggregate_lowers_to_a_wasm_artifact() {
    // variant shape { unit, count(s32) } -> a source data type
    let (module, bindings, resolve) = fixture(
        "package wasi:io@0.2.12; interface streams { variant shape { unit, count(s32) } get: func() -> shape; }",
        "get",
        vec![
            VariantCase {
                tag: 0,
                fields: Vec::new(),
            },
            VariantCase {
                tag: 1,
                fields: vec![erased()],
            },
        ],
    );
    lower_and_validate(module, bindings, resolve);
}

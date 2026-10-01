use super::*;

/// Lowers, optimizes, and validates a synthesized list fixture.
fn lower_and_validate_list(
    module: crate::cc::Module,
    bindings: crate::ExternalBindings,
    resolve: wit_parser::Resolve,
) {
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(module, bindings, target, registry)
        .expect("P9 should lower the aggregate list");
    let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the list ABI");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the aggregate list");
    let binary = crate::wasm::encode_module(&wasm).expect("the aggregate list Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the aggregate list Wasm should validate");
}

/// The element of the first `ListCopy` in `direction`, with its guest layout.
fn list_element(
    mir: &crate::mir::Module,
    direction: ListDirection,
) -> (CanonicalType, GuestLayout) {
    mir.functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::ListCopy {
                direction: found,
                element,
                element_guest,
                ..
            } if *found == direction => Some((element.clone(), element_guest.clone())),
            _ => None,
        })
        .expect("a matching ListCopy should be emitted")
}

#[test]
fn option_list_parameter_stores_the_variant_discriminant_and_payload() {
    let (cc, bindings, resolve) = option_string_list_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<option<string>> parameter");
    let (element, layout) = list_element(&mir, ListDirection::Store);
    assert!(matches!(element, CanonicalType::Option(_)));
    assert!(matches!(layout, GuestLayout::Variant { .. }));
    assert!(
        mir.functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(
                instruction,
                Instruction::ListCopy {
                    direction: ListDirection::Free,
                    ..
                }
            )),
        "the Just string buffer must be freed after the call"
    );
    let (cc, bindings, resolve) = option_string_list_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

#[test]
fn variant_list_parameter_stores_a_tagged_payload() {
    let (cc, bindings, resolve) = variant_list_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<variant> parameter");
    let (element, layout) = list_element(&mir, ListDirection::Store);
    assert!(matches!(element, CanonicalType::Variant(_)));
    assert!(matches!(layout, GuestLayout::Variant { .. }));
    let (cc, bindings, resolve) = variant_list_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

#[test]
fn nested_list_parameter_recurses_into_inner_buffers() {
    let (cc, bindings, resolve) = nested_list_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<list<s32>> parameter");
    let (element, layout) = list_element(&mir, ListDirection::Store);
    assert!(matches!(element, CanonicalType::List(_)));
    assert!(matches!(layout, GuestLayout::Array { .. }));
    let (cc, bindings, resolve) = nested_list_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

#[test]
fn option_list_result_loads_the_variant_payload() {
    let (cc, bindings, resolve) = option_string_list_result_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<option<string>> result");
    let (element, layout) = list_element(&mir, ListDirection::Load);
    assert!(matches!(element, CanonicalType::Option(_)));
    assert!(matches!(layout, GuestLayout::Variant { .. }));
    let (cc, bindings, resolve) = option_string_list_result_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

#[test]
fn fixed_list_parameter_copies_the_inline_elements() {
    let (cc, bindings, resolve) = fixed_list_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a fixed-length list parameter");
    let copies = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| matches!(instruction, Instruction::ArrayGet { .. }))
        .count();
    assert_eq!(
        copies, 3,
        "a fixed list of three elements reads each element"
    );
    let (cc, bindings, resolve) = fixed_list_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

#[test]
fn fixed_list_result_reads_the_inline_elements() {
    let (cc, bindings, resolve) = fixed_list_result_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a fixed-length list result");
    let (element, _) = list_element(&mir, ListDirection::Load);
    assert_eq!(
        element,
        CanonicalType::Int {
            width: 32,
            signed: true
        }
    );
    let (cc, bindings, resolve) = fixed_list_result_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

#[test]
fn wide_flags_list_element_packs_every_word() {
    let (cc, bindings, resolve) = wide_flags_list_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list of 33-flag elements");
    let (element, layout) = list_element(&mir, ListDirection::Store);
    assert!(matches!(&element, CanonicalType::Flags(names) if names.len() == 33));
    assert_eq!(abi::canonical::size_align(&element).size, 8);
    assert!(matches!(layout, GuestLayout::Product { .. }));
    let (cc, bindings, resolve) = wide_flags_list_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

#[test]
fn wide_flags_record_field_packs_every_word() {
    let (cc, bindings, resolve) = wide_flags_record_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list of records with a 33-flag field");
    let (element, layout) = list_element(&mir, ListDirection::Store);
    assert!(matches!(element, CanonicalType::Record(_)));
    assert!(matches!(layout, GuestLayout::Product { .. }));
    let (cc, bindings, resolve) = wide_flags_record_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

#[test]
fn wide_flags_variant_payload_packs_every_word() {
    let (cc, bindings, resolve) = wide_flags_variant_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list of option-33-flag elements");
    let (element, layout) = list_element(&mir, ListDirection::Store);
    assert!(matches!(element, CanonicalType::Option(_)));
    assert!(matches!(layout, GuestLayout::Variant { .. }));
    let (cc, bindings, resolve) = wide_flags_variant_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

#[test]
fn own_handle_list_result_copies_the_indices() {
    let (cc, bindings, resolve) = handle_result_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<own<T>> result");
    let (element, _) = list_element(&mir, ListDirection::Load);
    assert!(matches!(
        element,
        CanonicalType::Handle {
            ownership: crate::abi::canonical::Ownership::Own { .. },
            ..
        }
    ));
    let (cc, bindings, resolve) = handle_result_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

#[test]
fn option_of_nested_list_payload_recurses_and_frees() {
    let (cc, bindings, resolve) = option_list_list_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, _) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<option<list<s32>>> parameter");
    let (element, layout) = list_element(&mir, ListDirection::Store);
    assert!(matches!(element, CanonicalType::Option(_)));
    assert!(matches!(layout, GuestLayout::Variant { .. }));
    assert!(
        mir.functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(
                instruction,
                Instruction::ListCopy {
                    direction: ListDirection::Free,
                    ..
                }
            )),
        "the nested list buffer must be freed"
    );
    let (cc, bindings, resolve) = option_list_list_fixture();
    lower_and_validate_list(cc, bindings, resolve);
}

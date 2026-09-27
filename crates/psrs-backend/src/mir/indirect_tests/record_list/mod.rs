//! Synthesized `list<record>` fixtures and their MIR/Wasm tests. No in-scope
//! WASI interface exposes a list of records, so this drives the ABI path
//! directly, like the composite indirect-parameter fixture.

mod fixtures;
mod tests;

use super::lower_module_with_registry;
use crate::TargetCapabilities;
use crate::abi;
use crate::abi::canonical::CanonicalType;
use crate::abi::layout::SlotKind;
use crate::cc::GuestLayout;
use crate::mir::{Instruction, ListDirection};
use fixtures::{
    fixed_list_fixture, fixed_list_result_fixture, fixture, flags_fixture, handle_fixture,
    handle_result_fixture, nested_list_fixture, option_list_list_fixture,
    option_string_list_fixture, option_string_list_result_fixture, result_fixture, string_fixture,
    tuple_fixture, variant_list_fixture, wide_flags_list_fixture, wide_flags_record_fixture,
    wide_flags_variant_fixture,
};

#[test]
fn record_list_parameters_lower_to_a_canonical_layout() {
    let (cc, bindings, resolve) = fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<record> parameter");

    let (direction, element, layout) = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::ListCopy {
                direction,
                element,
                element_guest,
                ..
            } => Some((*direction, element.clone(), element_guest.clone())),
            _ => None,
        })
        .expect("a ListCopy should be emitted");
    assert_eq!(direction, ListDirection::Store);
    assert_eq!(
        abi::canonical::size_align(&element).size,
        16,
        "pair {{ x: s32, y: f64 }} is 16 bytes"
    );
    let CanonicalType::Record(fields) = &element else {
        panic!("the element should be a record, got {element:?}");
    };
    let offsets = crate::abi::layout::record_fields(
        fields
            .iter()
            .map(|field| crate::abi::layout::parameter_layout(&field.ty)),
    )
    .expect("the pair fields have a canonical layout");
    assert_eq!(
        offsets
            .iter()
            .map(|(offset, layout)| (*offset, layout.slots[0].kind))
            .collect::<Vec<_>>(),
        vec![(0, SlotKind::Word), (8, SlotKind::F64)]
    );
    let GuestLayout::Product { labels, .. } = &layout else {
        panic!("the element layout should be a product, got {layout:?}");
    };
    assert_eq!(labels, &["x".to_string(), "y".to_string()]);

    let mir =
        crate::mir::opt::optimize(mir, target).expect("P10 should preserve the record list copy");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the record list copy");
    let binary = crate::wasm::encode_module(&wasm).expect("the record list Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the record list Wasm should validate");
}

#[test]
fn record_list_results_rebuild_the_array() {
    let (cc, bindings, resolve) = result_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<record> result");
    let direction = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::ListCopy { direction, .. } => Some(*direction),
            _ => None,
        });
    assert_eq!(direction, Some(ListDirection::Load));

    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the record list result");
    let binary =
        crate::wasm::encode_module(&wasm).expect("the record list result Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the record list result Wasm should validate");
}

#[test]
fn record_list_string_fields_transcode_and_free() {
    let (cc, bindings, resolve) = string_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<record> with a string field");

    let copies = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            Instruction::ListCopy {
                direction,
                element,
                element_guest,
                ..
            } => Some((*direction, element.clone(), element_guest.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let (_, element, layout) = copies
        .iter()
        .find(|(direction, ..)| *direction == ListDirection::Store)
        .expect("a ListCopy Store should be emitted");
    assert_eq!(
        abi::canonical::size_align(element).size,
        12,
        "message {{ text: string, code: s32 }} is 12 bytes"
    );
    let CanonicalType::Record(fields) = element else {
        panic!("the element should be a record, got {element:?}");
    };
    let offsets = crate::abi::layout::record_fields(
        fields
            .iter()
            .map(|field| crate::abi::layout::parameter_layout(&field.ty)),
    )
    .expect("the message fields have a canonical layout");
    assert_eq!(
        fields
            .iter()
            .map(|field| field.ty.is_byte_list())
            .collect::<Vec<_>>(),
        vec![true, false]
    );
    assert_eq!(
        offsets
            .iter()
            .map(|(offset, layout)| (*offset, layout.slots[0].kind))
            .collect::<Vec<_>>(),
        vec![(0, SlotKind::Word), (8, SlotKind::Word)]
    );
    let GuestLayout::Product { labels, .. } = layout else {
        panic!("the element layout should be a product, got {layout:?}");
    };
    assert_eq!(labels, &["code".to_string(), "text".to_string()]);
    assert!(
        copies
            .iter()
            .any(|(direction, ..)| *direction == ListDirection::Free),
        "the string fields must be freed after the call"
    );

    let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the record list");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the record list with a string field");
    let binary = crate::wasm::encode_module(&wasm).expect("the record list Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the record list with a string field should validate");
}

#[test]
fn flags_list_packs_boolean_fields() {
    let (cc, bindings, resolve) = flags_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<flags> parameter");

    let (direction, element, layout) = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::ListCopy {
                direction,
                element,
                element_guest,
                ..
            } => Some((*direction, element.clone(), element_guest.clone())),
            _ => None,
        })
        .expect("a ListCopy should be emitted");
    assert_eq!(direction, ListDirection::Store);
    assert_eq!(
        abi::canonical::size_align(&element).size,
        1,
        "two flags pack into one canonical byte"
    );
    let CanonicalType::Flags(names) = &element else {
        panic!("the element should be flags, got {element:?}");
    };
    let GuestLayout::Product { labels, .. } = &layout else {
        panic!("the element layout should be a product, got {layout:?}");
    };
    assert_eq!(names, &["write".to_string(), "read".to_string()]);
    assert_eq!(labels, &["read".to_string(), "write".to_string()]);

    let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the flags copy");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the flags list copy");
    let binary = crate::wasm::encode_module(&wasm).expect("the flags list Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the flags list Wasm should validate");
}

#[test]
fn handle_list_parameters_copy_the_indexes() {
    let (cc, bindings, resolve) = handle_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<own<resource>> parameter");

    let (direction, element) = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::ListCopy {
                direction, element, ..
            } => Some((*direction, element.clone())),
            _ => None,
        })
        .expect("a ListCopy should be emitted");
    assert_eq!(direction, ListDirection::Store);
    assert!(matches!(element, CanonicalType::Handle { .. }));

    let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the handle copy");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the handle list copy");
    let binary = crate::wasm::encode_module(&wasm).expect("the handle list Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the handle list Wasm should validate");
}

#[test]
fn tuple_list_maps_to_a_record_list() {
    let (cc, bindings, resolve) = tuple_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower a list<tuple<string, string>> parameter");

    let (element, layout) = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::ListCopy {
                direction: ListDirection::Store,
                element,
                element_guest,
                ..
            } => Some((element.clone(), element_guest.clone())),
            _ => None,
        })
        .expect("a ListCopy Store should be emitted");
    assert_eq!(
        abi::canonical::size_align(&element).size,
        16,
        "two strings are 16 bytes"
    );
    let GuestLayout::Product { labels, fields, .. } = &layout else {
        panic!("the element layout should be a product, got {layout:?}");
    };
    assert_eq!(labels, &["_1".to_string(), "_2".to_string()]);
    assert_eq!(
        fields,
        &[crate::cc::ValueShape::String, crate::cc::ValueShape::String]
    );

    let mir = crate::mir::opt::optimize(mir, target).expect("P10 should preserve the tuple copy");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the tuple list copy");
    let binary = crate::wasm::encode_module(&wasm).expect("the tuple list Wasm should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the tuple list Wasm should validate");
}

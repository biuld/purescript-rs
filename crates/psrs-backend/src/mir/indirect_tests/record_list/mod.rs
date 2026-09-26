//! Synthesized `list<record>` fixtures and their MIR/Wasm tests. No in-scope
//! WASI interface exposes a list of records, so this drives the ABI path
//! directly, like the composite indirect-parameter fixture.

mod fixtures;

use super::lower_module_with_registry;
use crate::TargetCapabilities;
use crate::abi;
use fixtures::{fixture, result_fixture, string_fixture};

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

    let copy = mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            crate::mir::Instruction::ListCopyRecord {
                direction,
                size,
                fields,
                ..
            } => Some((*direction, *size, fields.clone())),
            _ => None,
        })
        .expect("a ListCopyRecord should be emitted");
    assert_eq!(copy.0, crate::mir::ListDirection::Store);
    assert_eq!(copy.1, 16, "pair {{ x: s32, y: f64 }} is 16 bytes");
    let layout = copy
        .2
        .iter()
        .map(|field| match field {
            crate::mir::ListFieldCopy::Scalar {
                offset,
                index,
                kind,
            } => (*offset, *index, *kind),
            other => panic!("expected a scalar field, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        layout,
        vec![
            (0, 0, crate::abi::layout::SlotKind::Word),
            (8, 1, crate::abi::layout::SlotKind::F64),
        ]
    );

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
            crate::mir::Instruction::ListCopyRecord { direction, .. } => Some(*direction),
            _ => None,
        });
    assert_eq!(direction, Some(crate::mir::ListDirection::Load));

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
            crate::mir::Instruction::ListCopyRecord {
                direction,
                size,
                fields,
                ..
            } => Some((*direction, *size, fields.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let store = copies
        .iter()
        .find(|(direction, ..)| *direction == crate::mir::ListDirection::Store)
        .expect("a ListCopyRecord Store should be emitted");
    assert_eq!(
        store.1, 12,
        "message {{ text: string, code: s32 }} is 12 bytes"
    );
    assert_eq!(
        store.2,
        vec![
            crate::mir::ListFieldCopy::String {
                offset: 0,
                index: 1,
            },
            crate::mir::ListFieldCopy::Scalar {
                offset: 8,
                index: 0,
                kind: crate::abi::layout::SlotKind::Word,
            },
        ]
    );
    assert!(
        copies
            .iter()
            .any(|(direction, ..)| *direction == crate::mir::ListDirection::FreeStrings),
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

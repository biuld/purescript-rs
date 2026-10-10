//! ABI arrays and records cross bare slots in both directions.
use super::*;
use crate::cc::{
    Assignment, AssignmentKind, External, ExternalProjection, GuestLayout, Signature, ValueDecl,
};
use crate::types::ValueId;
use psrs_hir::SymbolId;

fn roundtrip(array: bool, include_protocol: bool) -> (cc::Module, ExternalBindings, Resolve) {
    let (mut module, mut bindings, _) = if array {
        option_list_record_fixture()
    } else {
        nested_record_fixture()
    };
    let result_shape = module.externals[0].signature.as_ref().unwrap().result;
    let mut guest = cc::guest_layout(result_shape, &module.representations).unwrap();
    let GuestLayout::Variant { repr, cases } = &mut guest else {
        panic!("variant fixture")
    };
    cases[1].fields[0].stored = erased();
    let Representation::Variant { cases: stored } =
        &mut module.representations.representations[repr.0 as usize]
    else {
        panic!("variant storage")
    };
    stored[1].fields[0] = erased();
    // The canonical protocol is registered by CC's layout owner. It is only
    // reachable through ABI storage conversion, never through a source value.
    if include_protocol {
        let products = module.representations.product_labels.clone();
        for (_, labels) in products {
            let id = module.representations.reserve();
            module.representations.set(
                id,
                Representation::Product {
                    fields: vec![erased(); labels.len()],
                },
            );
            module.representations.set_product_labels(id, labels);
        }
        if array {
            let id = module.representations.reserve();
            module
                .representations
                .set(id, Representation::Array { element: erased() });
        }
    }
    let id = module.representations.reserve();
    module.representations.set(
        id,
        Representation::Box {
            value: ValueShape::Integer,
        },
    );
    module.externals[0].projection = Some(ExternalProjection {
        parameters: vec![],
        result: Some(guest.clone()),
    });
    let get = module.externals[0].symbol;
    let take = SymbolId::new(get.module, get.index + 1);
    module.externals.push(External {
        symbol: take,
        signature: Some(Signature {
            parameters: vec![result_shape],
            result: ValueShape::Integer,
        }),
        projection: Some(ExternalProjection {
            parameters: vec![guest],
            result: None,
        }),
    });
    let mut binding = bindings.imports[0].clone();
    binding.symbol = take;
    binding.function = "take".into();
    bindings.imports.push(binding);
    let main = &mut module.functions[0];
    main.values.push(ValueDecl {
        id: ValueId(3),
        ty: ValueShape::Integer,
    });
    main.assignments.push(Assignment {
        destination: ValueId(3),
        kind: AssignmentKind::DirectCall {
            function: take,
            arguments: vec![ValueId(0)],
        },
        span: module.span,
    });
    main.result = ValueId(3);
    let payload = if array { "list<pair>" } else { "pair" };
    let mut resolve = Resolve::default();
    resolve.push_str("storage.wit", &format!(
        "package wasi:io@0.2.12; interface streams {{ record pair {{ x: s32, y: bool }} get: func() -> option<{payload}>; take: func(value: option<{payload}>) -> s32; }}"
    )).unwrap();
    (module, bindings, resolve)
}

#[test]
fn erased_record_array_payload_roundtrips_through_canonical_storage() {
    let (module, bindings, resolve) = roundtrip(true, true);
    lower_and_validate(module, bindings, resolve);
}

#[test]
fn erased_record_payload_roundtrips_through_canonical_storage() {
    let (module, bindings, resolve) = roundtrip(false, true);
    lower_and_validate(module, bindings, resolve);
}

#[test]
fn erased_payload_without_an_owner_protocol_is_rejected() {
    let (module, bindings, resolve) = roundtrip(true, false);
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let Err(errors) = lower_module_with_registry(module, bindings, target, registry) else {
        panic!("a missing storage protocol must be rejected");
    };
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("storage protocol")),
        "{errors:?}"
    );
}

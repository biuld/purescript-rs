//! Conformance and unit tests for the normalized canonical ABI model.

use super::*;
use wit_parser::abi::{AbiVariant, WasmType};
use wit_parser::{Function, Resolve};

fn core_val(ty: WasmType) -> CoreVal {
    match ty {
        WasmType::I32 | WasmType::Pointer | WasmType::Length => CoreVal::I32,
        WasmType::I64 | WasmType::PointerOrI64 => CoreVal::I64,
        WasmType::F32 => CoreVal::F32,
        WasmType::F64 => CoreVal::F64,
    }
}

/// Asserts that the model's derived core signature equals the canonical
/// signature `wit_parser` reports, which is the oracle the componentizer uses.
fn assert_matches_oracle(resolve: &Resolve, function: &Function) {
    let abi = function_abi(resolve, function).expect("function should be representable");
    let signature = resolve.wasm_signature(AbiVariant::GuestImport, function);

    let mut expected_params = if abi.indirect_params {
        vec![CoreVal::I32]
    } else {
        abi.flat_params.clone()
    };
    if abi.retptr {
        expected_params.push(CoreVal::I32);
    }
    let expected_results = if abi.retptr {
        Vec::new()
    } else {
        abi.flat_results.clone()
    };
    let actual_params = signature
        .params
        .iter()
        .copied()
        .map(core_val)
        .collect::<Vec<_>>();
    let actual_results = signature
        .results
        .iter()
        .copied()
        .map(core_val)
        .collect::<Vec<_>>();

    assert_eq!(
        expected_params, actual_params,
        "params of `{}`",
        function.name
    );
    assert_eq!(
        expected_results, actual_results,
        "results of `{}`",
        function.name
    );
    assert_eq!(
        abi.indirect_params, signature.indirect_params,
        "indirect_params of `{}`",
        function.name
    );
    assert_eq!(
        abi.retptr, signature.retptr,
        "retptr of `{}`",
        function.name
    );
}

#[test]
fn flatten_matches_wasm_signature_for_every_wasi_function() {
    let mut resolve = Resolve::default();
    crate::abi::load_wit(&mut resolve).expect("vendored WASI should load");

    let mut checked = 0;
    for (_, interface) in resolve.interfaces.iter() {
        for function in interface.functions.values() {
            if function_abi(&resolve, function).is_none() {
                continue;
            }
            assert_matches_oracle(&resolve, function);
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "the vendored WASI surface should have representable functions"
    );
}

#[test]
fn flatten_joins_variant_payloads_like_the_oracle() {
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str(
            "join.wit",
            r#"
package test:join;
interface shapes {
    variant tagged { a(u32), b(f64) }
    mixed: func(x: result<u64, f32>) -> result<u64, f32>;
    nested: func(x: list<option<string>>) -> list<option<string>>;
    tagged-fn: func(x: tagged) -> tagged;
}
"#,
        )
        .expect("test WIT should parse");
    let interface = resolve.packages[package].interfaces["shapes"];
    for function in resolve.interfaces[interface].functions.values() {
        assert_matches_oracle(&resolve, function);
    }

    let mixed = &resolve.interfaces[interface].functions["mixed"];
    let abi = function_abi(&resolve, mixed).expect("mixed is representable");
    assert_eq!(abi.flat_params, vec![CoreVal::I32, CoreVal::I64]);
    assert!(abi.retptr);

    let nested = &resolve.interfaces[interface].functions["nested"];
    let abi = function_abi(&resolve, nested).expect("nested is representable");
    assert_eq!(abi.flat_params, vec![CoreVal::I32, CoreVal::I32]);
}

#[test]
fn size_align_matches_the_canonical_layout() {
    let u8 = CanonicalType::int(8, false);
    let u64 = CanonicalType::int(64, false);

    assert_eq!(
        size_align(&CanonicalType::Bool),
        SizeAlign { size: 1, align: 1 }
    );
    assert_eq!(size_align(&u64), SizeAlign { size: 8, align: 8 });
    assert_eq!(
        size_align(&CanonicalType::String),
        SizeAlign { size: 8, align: 4 }
    );

    let record = CanonicalType::Record(vec![
        CanonicalField {
            name: "a".into(),
            ty: u8.clone(),
        },
        CanonicalField {
            name: "b".into(),
            ty: u64.clone(),
        },
    ]);
    assert_eq!(size_align(&record), SizeAlign { size: 16, align: 8 });

    let option = CanonicalType::Option(Box::new(u64.clone()));
    assert_eq!(size_align(&option), SizeAlign { size: 16, align: 8 });

    let result = CanonicalType::Result {
        ok: None,
        err: Some(Box::new(CanonicalType::String)),
    };
    assert_eq!(size_align(&result), SizeAlign { size: 12, align: 4 });

    let fixed = CanonicalType::FixedList {
        element: Box::new(u8.clone()),
        length: 3,
    };
    assert_eq!(size_align(&fixed), SizeAlign { size: 3, align: 1 });

    let flags = CanonicalType::Flags((0..33).map(|index| format!("f{index}")).collect());
    assert_eq!(size_align(&flags), SizeAlign { size: 8, align: 4 });
}

#[test]
fn result_area_sizes_a_unit_success_from_its_error_payload() {
    let unit = CanonicalType::Result {
        ok: None,
        err: Some(Box::new(CanonicalType::String)),
    };
    assert_eq!(crate::abi::layout::result_area(&unit), Some((12, 4)));

    let big = CanonicalType::Record(vec![
        CanonicalField {
            name: "a".into(),
            ty: CanonicalType::Float { width: 64 },
        },
        CanonicalField {
            name: "b".into(),
            ty: CanonicalType::Float { width: 64 },
        },
        CanonicalField {
            name: "c".into(),
            ty: CanonicalType::int(64, true),
        },
    ]);
    let large = CanonicalType::Result {
        ok: None,
        err: Some(Box::new(big)),
    };
    assert_eq!(crate::abi::layout::result_area(&large), Some((32, 8)));
}

#[test]
fn despecialize_is_idempotent_and_preserves_flatten_and_size() {
    let ty = CanonicalType::List(Box::new(CanonicalType::Option(Box::new(
        CanonicalType::String,
    ))));
    let special = despecialize(&ty);
    assert_eq!(&despecialize(&special), &special);
    assert_eq!(flatten(&ty), flatten(&special));
    assert_eq!(size_align(&ty), size_align(&special));
}

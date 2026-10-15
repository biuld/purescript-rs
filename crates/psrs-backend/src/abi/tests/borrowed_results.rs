//! WIT resolution owns recursive borrowed-result rejection. The compiler must
//! not erase that rule into a successful-looking source value. Parameters keep
//! their borrowing metadata; owned results retain the explicit-drop contract.
use super::super::{
    WasiRegistry,
    canonical::{CanonicalType as T, Ownership},
};

fn source(result: &str, parameter: bool, owned: bool) -> String {
    let ownership = if owned { "own" } else { "borrow" };
    let signature = if parameter {
        format!("use-value: func(value: {result});")
    } else {
        format!("get: func() -> {result};")
    };
    format!(
        "package wasi:io@0.2.12; interface streams {{ resource file; record payload {{ value: {ownership}<file> }} variant envelope {{ absent, value({ownership}<file>) }} {signature} }}"
    )
}

#[test]
fn wit_resolution_rejects_borrowed_results_at_every_supported_nesting_form() {
    for payload in [
        "borrow<file>",
        "payload",
        "envelope",
        "tuple<u32, payload>",
        "list<payload>",
    ] {
        for result in [
            payload.to_owned(),
            format!("option<{payload}>"),
            format!("result<{payload}, u32>"),
            format!("result<u32, {payload}>"),
            format!("option<tuple<u32, {payload}>>"),
        ] {
            let mut resolve = wit_parser::Resolve::default();
            let error = resolve
                .push_str("borrow-result.wit", &source(&result, false, false))
                .expect_err(
                    "borrowed result must fail WIT resolution before canonical/source erasure",
                );
            let message = format!("{error:#}");
            assert!(
                message.contains("function returns a type which contains a `borrow<T>`"),
                "{result}: unexpected rejection owner: {message}"
            );
        }
    }
}

fn registry(ty: &str, parameter: bool, owned: bool) -> WasiRegistry {
    let mut resolve = wit_parser::Resolve::default();
    resolve
        .push_str("resource-boundary.wit", &source(ty, parameter, owned))
        .expect("positive WIT declaration must resolve");
    WasiRegistry::from_resolve(resolve, crate::TargetCapabilities::default())
}

fn contains_borrow(ty: &T) -> bool {
    match ty {
        T::Handle {
            ownership: Ownership::Borrow,
            ..
        } => true,
        T::Record(fields) => fields.iter().any(|field| contains_borrow(&field.ty)),
        T::Variant(cases) => cases
            .iter()
            .any(|case| case.payload.as_deref().is_some_and(contains_borrow)),
        T::Option(payload) => contains_borrow(payload),
        T::Result { ok, err } => {
            ok.as_deref().is_some_and(contains_borrow)
                || err.as_deref().is_some_and(contains_borrow)
        }
        _ => false,
    }
}

#[test]
fn borrowed_parameters_preserve_their_scoped_canonical_metadata() {
    for ty in [
        "borrow<file>",
        "payload",
        "envelope",
        "option<payload>",
        "result<payload, u32>",
        "tuple<u32, payload>",
    ] {
        let mut registry = registry(ty, true, false);
        let import = registry.import("wasi:io/streams", "use-value").unwrap();
        assert_eq!(import.unsupported, None, "{ty}");
        assert_eq!(import.params.len(), 1);
        assert!(
            contains_borrow(&import.params[0]),
            "borrow metadata disappeared inside {ty}"
        );
    }
}

#[test]
fn owned_aggregate_results_remain_supported_with_explicit_drops() {
    for ty in [
        "own<file>",
        "payload",
        "envelope",
        "option<payload>",
        "result<payload, u32>",
        "tuple<u32, payload>",
    ] {
        let mut registry = registry(ty, false, true);
        let import = registry.import("wasi:io/streams", "get").unwrap();
        assert_eq!(import.unsupported, None, "{ty}");
        assert!(!contains_borrow(
            import.canonical_result.as_ref().expect("owned result")
        ));
        let drop = registry
            .import("wasi:io/streams", "[resource-drop]file")
            .unwrap();
        assert_eq!(drop.unsupported, None);
    }
}

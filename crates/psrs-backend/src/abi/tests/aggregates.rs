//! DEC-13 aggregate mappings: WIT `option`/`result`/`variant` recognized as
//! `Data.Maybe.Maybe`, `Data.Either.Either`, and a source data type.

use super::canonical::{CanonicalCase, CanonicalType, resolve as canonical_resolve};
use super::test_support::import;
use super::*;
use crate::cc::{RefShape, Reference, ReprId, ValueShape};
use psrs_core::{ConstructorInfo, Type as CoreType, TypeConstructor, TypeId as CoreTypeId};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId};
use std::collections::HashMap;

fn user(index: u32) -> HirTypeId {
    HirTypeId::new(ModuleId(0), index)
}

fn constructor(
    index: u32,
    name: &str,
    type_id: HirTypeId,
    tag: u32,
    field_types: Vec<CoreTypeId>,
) -> ConstructorInfo {
    ConstructorInfo {
        symbol: SymbolId::new(ModuleId(0), index),
        name: name.into(),
        type_id,
        tag,
        field_count: field_types.len(),
        field_types,
        parameters: Vec::new(),
    }
}

/// A module with `Data.Maybe.Maybe`, `Data.Either.Either`, and a local `Shape`
/// whose Core types and constructors are laid out for validation. Core type 0
/// is `I32` and type 1 is `String`.
fn aggregate_module() -> psrs_core::Module {
    let mut module = empty_core_module();
    let maybe = user(0);
    let either = user(1);
    let shape = user(2);
    module.type_names = vec![
        (maybe, "Data.Maybe.Maybe".into()),
        (either, "Data.Either.Either".into()),
        (shape, "Main.Shape".into()),
    ];
    let _ = intern_all(
        &mut module,
        vec![
            CoreType::I32,
            CoreType::String,
            CoreType::Constructor(TypeConstructor::User(shape)),
        ],
    );
    module.constructors = vec![
        constructor(0, "Nothing", maybe, 0, vec![]),
        constructor(1, "Just", maybe, 1, vec![CoreTypeId(0)]),
        constructor(2, "Left", either, 0, vec![CoreTypeId(0)]),
        constructor(3, "Right", either, 1, vec![CoreTypeId(0)]),
        constructor(4, "Unit", shape, 0, vec![]),
        constructor(5, "Count", shape, 1, vec![CoreTypeId(0)]),
        constructor(6, "Label", shape, 2, vec![CoreTypeId(1)]),
    ];
    module
}

fn append(module: &mut psrs_core::Module, ty: CoreType) -> CoreTypeId {
    intern_all(module, vec![ty]).pop().expect("one type")
}

fn option_type() -> CanonicalType {
    CanonicalType::Option(Box::new(CanonicalType::String))
}

fn result_type() -> CanonicalType {
    CanonicalType::Result {
        ok: Some(Box::new(CanonicalType::Int {
            width: 32,
            signed: false,
        })),
        err: Some(Box::new(CanonicalType::String)),
    }
}

fn variant_type() -> CanonicalType {
    CanonicalType::Variant(vec![
        CanonicalCase {
            name: "unit".into(),
            payload: None,
        },
        CanonicalCase {
            name: "count".into(),
            payload: Some(Box::new(CanonicalType::Int {
                width: 32,
                signed: true,
            })),
        },
        CanonicalCase {
            name: "label".into(),
            payload: Some(Box::new(CanonicalType::String)),
        },
    ])
}

fn aggregate_import(name: &str, params: Vec<CanonicalType>) -> WasiImport {
    import(
        SymbolId::new(ModuleId::INTRINSICS, 0),
        "test:agg",
        name,
        params,
        None,
    )
}

#[test]
fn classifies_wit_option_result_and_variant() {
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str(
            "aggregates.wit",
            "package wasi:io@0.2.12; interface streams { variant shape { unit, count(s32), label(string) } send: func(o: option<string>, r: result<u32, string>, v: shape); }",
        )
        .expect("the aggregate WIT fixture should resolve");
    let interface = resolve.packages[package].interfaces["streams"];
    let send = &resolve.interfaces[interface].functions["send"];

    assert_eq!(
        canonical_resolve(&resolve, &send.params[0].ty),
        Some(option_type())
    );
    assert_eq!(
        canonical_resolve(&resolve, &send.params[1].ty),
        Some(result_type())
    );
    assert_eq!(
        canonical_resolve(&resolve, &send.params[2].ty),
        Some(variant_type())
    );
}

#[test]
fn validates_option_result_and_variant_parameters() {
    let mut module = aggregate_module();
    let maybe_head = append(
        &mut module,
        CoreType::Constructor(TypeConstructor::User(user(0))),
    );
    let either_head = append(
        &mut module,
        CoreType::Constructor(TypeConstructor::User(user(1))),
    );
    let maybe_string = append(
        &mut module,
        CoreType::Application(maybe_head, CoreTypeId(1)),
    );
    let either_int = append(
        &mut module,
        CoreType::Application(either_head, CoreTypeId(0)),
    );
    let either_int_string = append(
        &mut module,
        CoreType::Application(either_int, CoreTypeId(1)),
    );
    let shape = append(
        &mut module,
        CoreType::Constructor(TypeConstructor::User(user(2))),
    );
    let unit = append(&mut module, CoreType::Unit);

    let option = aggregate_import("option", vec![option_type()]);
    let function = function_type(&mut module, &[maybe_string], unit);
    crate::abi::link::validate_import_signature(&option, &module, function)
        .expect("Maybe String should match WIT option<string>");

    let result = aggregate_import("result", vec![result_type()]);
    let function = function_type(&mut module, &[either_int_string], unit);
    crate::abi::link::validate_import_signature(&result, &module, function)
        .expect("Either Int String should match WIT result<u32, string>");

    let variant = aggregate_import("variant", vec![variant_type()]);
    let function = function_type(&mut module, &[shape], unit);
    crate::abi::link::validate_import_signature(&variant, &module, function)
        .expect("Shape should match the WIT variant in case order");

    // A payload whose Core shape disagrees with the WIT case order is rejected.
    let wrong = CanonicalType::Variant(vec![
        CanonicalCase {
            name: "unit".into(),
            payload: Some(Box::new(CanonicalType::Int {
                width: 32,
                signed: true,
            })),
        },
        CanonicalCase {
            name: "count".into(),
            payload: None,
        },
        CanonicalCase {
            name: "label".into(),
            payload: Some(Box::new(CanonicalType::String)),
        },
    ]);
    let wrong = aggregate_import("variant", vec![wrong]);
    let function = function_type(&mut module, &[shape], unit);
    assert!(crate::abi::link::validate_import_signature(&wrong, &module, function).is_err());
}

#[test]
fn validates_a_unit_success_result_as_either_unit() {
    // DEC-13: `result<_, E>` maps to `Either Unit E`, not to a trapping `Unit`.
    let mut module = aggregate_module();
    let unit = append(&mut module, CoreType::Unit);
    let either_head = append(
        &mut module,
        CoreType::Constructor(TypeConstructor::User(user(1))),
    );
    let either_unit = append(&mut module, CoreType::Application(either_head, unit));
    let either_unit_int = append(
        &mut module,
        CoreType::Application(either_unit, CoreTypeId(0)),
    );
    let unit_result = CanonicalType::Result {
        ok: None,
        err: Some(Box::new(int(32, false))),
    };
    let import = import(
        SymbolId::new(ModuleId::INTRINSICS, 0),
        "test:agg",
        "unit-result",
        Vec::new(),
        Some(unit_result),
    );

    let function = function_type(&mut module, &[], either_unit_int);
    crate::abi::link::validate_import_signature(&import, &module, function)
        .expect("Either Unit Int should match WIT result<_, u32>");

    // The removed `Unit`/trap mapping is no longer accepted.
    let function = function_type(&mut module, &[], unit);
    assert!(
        crate::abi::link::validate_import_signature(&import, &module, function).is_err(),
        "a bare Unit result is not the mapped Either"
    );
}

#[test]
fn cc_recognizes_a_payload_bearing_data_type_as_a_variant_reference() {
    let mut module = aggregate_module();
    let maybe_head = append(
        &mut module,
        CoreType::Constructor(TypeConstructor::User(user(0))),
    );
    let maybe_string = append(
        &mut module,
        CoreType::Application(maybe_head, CoreTypeId(1)),
    );
    let function = function_type(&mut module, &[maybe_string], CoreTypeId(0));
    let repr = ReprId(7);
    let constructor_types = HashMap::from([(SymbolId::new(ModuleId(0), 1), repr)]);
    let signature = crate::cc::abstract_signature(
        Some(function),
        &module,
        &HashMap::new(),
        &HashMap::new(),
        &constructor_types,
    )
    .expect("Maybe String should have an abstract signature");
    assert_eq!(
        signature.parameters,
        vec![ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(repr),
        })]
    );
}

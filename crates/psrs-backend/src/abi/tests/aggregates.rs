//! DEC-13 aggregate mappings: WIT `option`/`result`/`variant` recognized as
//! `Data.Maybe.Maybe`, `Data.Either.Either`, and a source data type.

use super::*;
use crate::abi::WasiVariantCase;
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

fn option_import() -> WasiImport {
    WasiImport {
        symbol: SymbolId::new(ModuleId::INTRINSICS, 0),
        module: "test:agg".into(),
        name: "option".into(),
        parameters: vec![ValueType::I32, ValueType::I32, ValueType::I32],
        param_kinds: vec![WasiParamKind::Option {
            payload: Box::new(WasiParamKind::List),
        }],
        result: None,
        result_kind: WasiResultKind::None,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    }
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
        param_kind(&resolve, &send.params[0].ty),
        WasiParamKind::Option {
            payload: Box::new(WasiParamKind::List),
        }
    );
    assert_eq!(
        param_kind(&resolve, &send.params[1].ty),
        WasiParamKind::Result {
            ok: Box::new(WasiParamKind::Integer32),
            err: Box::new(WasiParamKind::List),
        }
    );
    assert_eq!(
        param_kind(&resolve, &send.params[2].ty),
        WasiParamKind::Variant {
            cases: vec![
                WasiVariantCase {
                    name: "Unit".into(),
                    kind: None,
                },
                WasiVariantCase {
                    name: "Count".into(),
                    kind: Some(Box::new(WasiParamKind::Integer32)),
                },
                WasiVariantCase {
                    name: "Label".into(),
                    kind: Some(Box::new(WasiParamKind::List)),
                },
            ],
        }
    );
    assert_eq!(
        result_kind(&resolve, &send.params[1].ty),
        WasiResultKind::ValueResult {
            ok: Box::new(WasiParamKind::Integer32),
            err: Box::new(WasiParamKind::List),
        }
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

    let option = option_import();
    let function = function_type(&mut module, &[maybe_string], unit);
    crate::abi::link::validate_import_signature(&option, &module, function)
        .expect("Maybe String should match WIT option<string>");

    let result = WasiImport {
        param_kinds: vec![WasiParamKind::Result {
            ok: Box::new(WasiParamKind::Integer32),
            err: Box::new(WasiParamKind::List),
        }],
        ..option.clone()
    };
    let function = function_type(&mut module, &[either_int_string], unit);
    crate::abi::link::validate_import_signature(&result, &module, function)
        .expect("Either Int String should match WIT result<u32, string>");

    let variant = WasiImport {
        param_kinds: vec![WasiParamKind::Variant {
            cases: vec![
                WasiVariantCase {
                    name: "Unit".into(),
                    kind: None,
                },
                WasiVariantCase {
                    name: "Count".into(),
                    kind: Some(Box::new(WasiParamKind::Integer32)),
                },
                WasiVariantCase {
                    name: "Label".into(),
                    kind: Some(Box::new(WasiParamKind::List)),
                },
            ],
        }],
        ..option.clone()
    };
    let function = function_type(&mut module, &[shape], unit);
    crate::abi::link::validate_import_signature(&variant, &module, function)
        .expect("Shape should match the WIT variant in case order");

    // A payload whose Core shape disagrees with the WIT case order is rejected.
    let wrong = WasiImport {
        param_kinds: vec![WasiParamKind::Variant {
            cases: vec![
                WasiVariantCase {
                    name: "Unit".into(),
                    kind: Some(Box::new(WasiParamKind::Integer32)),
                },
                WasiVariantCase {
                    name: "Count".into(),
                    kind: None,
                },
                WasiVariantCase {
                    name: "Label".into(),
                    kind: Some(Box::new(WasiParamKind::List)),
                },
            ],
        }],
        ..variant.clone()
    };
    let function = function_type(&mut module, &[shape], unit);
    assert!(crate::abi::link::validate_import_signature(&wrong, &module, function).is_err());
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

//! Primitive imports of WIT aggregates. The fixture style matches
//! `crate::abi::tests`: a local WIT package, not an edit to the application world.

use super::common::RecordingLowerer;
use super::*;
use crate::abi::{FlatSlot, WasiImport, WasiRegistry, WasiResultKind};
use crate::capability::TargetCapabilities;
use crate::cc::ValueShape;
use psrs_core::{Type as CoreType, TypeId as CoreTypeId};
use psrs_hir::{
    BuiltinType, ModuleId, Type as HirType, TypeId as HirTypeId, TypeKind as HirTypeKind,
};
use psrs_span::TextRange;
use wit_parser::abi::{AbiVariant, WasmType};

fn span() -> TextRange {
    TextRange::new(0, 4)
}

/// Validates a declaration whose parameters and result are the given Core types
/// against the resolved WIT descriptor, using the production conformance check.
fn validate(
    import: &WasiImport,
    parameters: Vec<CoreType>,
    result: CoreType,
) -> Result<(), String> {
    let mut module = psrs_core::Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: Vec::new(),
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: span(),
    };
    let mut parameter_ids = Vec::new();
    for ty in parameters {
        module.types.push(ty);
        parameter_ids.push(CoreTypeId((module.types.len() - 1) as u32));
    }
    module.types.push(result);
    let mut current = CoreTypeId((module.types.len() - 1) as u32);
    for parameter in parameter_ids.iter().rev() {
        module.types.push(CoreType::Function {
            parameter: *parameter,
            result: current,
        });
        current = CoreTypeId((module.types.len() - 1) as u32);
    }
    crate::abi::link::validate_import_signature(import, &module, current)
}

/// The CC abstract signature used by MIR lowering.
fn cc_signature(parameters: Vec<ValueShape>) -> crate::cc::Signature {
    crate::cc::Signature {
        parameters,
        result: ValueShape::Integer,
    }
}

#[test]
fn option_string_validates_and_lowers_as_a_discriminant_and_string() {
    let mut resolve = wit_parser::Resolve::default();
    let package = resolve
        .push_str(
            "option.wit",
            "package wasi:io@0.2.12; interface streams { resource output-stream; send: func(value: option<string>); read: func() -> option<string>; take: func(value: option<borrow<output-stream>>); }",
        )
        .expect("the option WIT fixture should resolve");
    let interface = resolve.packages[package].interfaces["streams"];
    let send = &resolve.interfaces[interface].functions["send"];
    let canonical = resolve.wasm_signature(AbiVariant::GuestImport, send);
    assert_eq!(
        canonical.params,
        vec![WasmType::I32, WasmType::Pointer, WasmType::Length]
    );
    assert!(!canonical.indirect_params);
    assert!(!canonical.retptr);

    let mut registry = WasiRegistry::from_resolve(resolve, TargetCapabilities::default());
    let import = registry
        .import("wasi:io/streams", "send")
        .expect("send should resolve");
    assert!(import.unsupported.is_none());
    assert_eq!(
        import.flat_slots,
        vec![FlatSlot::Int32, FlatSlot::Pointer, FlatSlot::Length]
    );
    validate(
        &import,
        vec![CoreType::I32, CoreType::String],
        CoreType::Unit,
    )
    .expect("option<string> flattens to Int -> String -> Unit");

    assert!(
        validate(
            &import,
            vec![CoreType::Char, CoreType::String],
            CoreType::Unit,
        )
        .is_err(),
        "Char is not the option discriminant"
    );
    assert!(validate(&import, vec![CoreType::Record(vec![])], CoreType::Unit).is_err());

    let named = HirType {
        kind: HirTypeKind::Function {
            parameter: Box::new(HirType {
                kind: HirTypeKind::Named(HirTypeId::new(ModuleId(0), 0)),
                span: span(),
            }),
            result: Box::new(HirType {
                kind: HirTypeKind::Constructor(BuiltinType::Unit),
                span: span(),
            }),
        },
        span: span(),
    };
    let mut module = psrs_core::Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: Vec::new(),
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: span(),
    };
    assert!(
        crate::abi::intern_source_type(&mut module, &named).is_none(),
        "a non-primitive source type such as Maybe String has no source mapping"
    );

    let mut lowerer = RecordingLowerer::default();
    let discriminant = ValueId(30);
    let text = ValueId(31);
    lower(
        &mut lowerer,
        &import,
        &cc_signature(vec![ValueShape::Integer, ValueShape::String]),
        ValueId(7),
        &[discriminant, text],
        span(),
        BlockId(0),
    )
    .expect("Int -> String -> Unit should lower");
    assert!(
        lowerer.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                Instruction::Call { function, .. } if *function == crate::abi::STRING_TO_BYTES_SYMBOL
            )
        }),
        "String still lowers to a pointer and a length: {:?}",
        lowerer.instructions
    );
    assert!(
        lowerer.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                Instruction::CallVoid { function, arguments, .. }
                    if *function == import.symbol && arguments.first() == Some(&discriminant) && arguments.len() == 3
            )
        }),
        "the call is a discriminant plus the string's pointer and length: {:?}",
        lowerer.instructions
    );

    let read = registry
        .import("wasi:io/streams", "read")
        .expect("read should resolve");
    assert_eq!(read.result_kind, WasiResultKind::Discarded);
    assert!(
        read.unsupported
            .as_deref()
            .is_some_and(|message| message.contains("result"))
    );
    assert!(
        validate(&read, Vec::new(), CoreType::String).is_err(),
        "a multi-value canonical result is not one primitive"
    );

    let take = registry
        .import("wasi:io/streams", "take")
        .expect("take should resolve");
    assert_eq!(take.flat_slots, vec![FlatSlot::Int32, FlatSlot::Handle]);
    validate(&take, vec![CoreType::I32, CoreType::I32], CoreType::Unit)
        .expect("a handle payload is declared as Int");
    assert!(validate(&take, vec![CoreType::Char, CoreType::I32], CoreType::Unit,).is_err());
    assert!(
        validate(&take, vec![CoreType::I32, CoreType::Char], CoreType::Unit,).is_err(),
        "a handle slot is not a Char"
    );
}

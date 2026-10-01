//! Primitive imports of WIT aggregates. The fixture style matches
//! `crate::abi::tests`: a local WIT package, not an edit to the application world.

use super::common::RecordingLowerer;
use super::*;
use crate::abi::canonical::{CanonicalType, FlatLeaf, Ownership, flat_leaves_of};
use crate::abi::{WasiImport, WasiRegistry};
use crate::capability::TargetCapabilities;
use crate::cc::ValueShape;
use psrs_core::{Type as CoreType, TypeConstructor as CoreTypeConstructor, TypeId as CoreTypeId};
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
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: Vec::new(),
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
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
        current = push_core_arrow(&mut module, *parameter, current);
    }
    crate::abi::link::validate_import_signature(import, &module, current)
}

/// Appends `parameter -> result` as the application spine and returns its id.
fn push_core_arrow(
    module: &mut psrs_core::Module,
    parameter: CoreTypeId,
    result: CoreTypeId,
) -> CoreTypeId {
    let head = CoreTypeId(module.types.len() as u32);
    module
        .types
        .push(CoreType::Constructor(CoreTypeConstructor::Function));
    let inner = CoreTypeId(module.types.len() as u32);
    module.types.push(CoreType::Application(head, parameter));
    let outer = CoreTypeId(module.types.len() as u32);
    module.types.push(CoreType::Application(inner, result));
    outer
}

/// The CC abstract signature used by MIR lowering.
fn cc_signature(parameters: Vec<ValueShape>) -> crate::cc::Signature {
    crate::cc::Signature {
        parameters,
        result: ValueShape::Integer,
    }
}

fn option(inner: CanonicalType) -> CanonicalType {
    CanonicalType::Option(Box::new(inner))
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
    assert_eq!(import.params, vec![option(CanonicalType::String)]);
    assert_eq!(
        flat_leaves_of(&import.params),
        vec![FlatLeaf::Int32, FlatLeaf::Pointer, FlatLeaf::Length]
    );
    validate(
        &import,
        vec![
            CoreType::Constructor(psrs_core::TypeConstructor::Int),
            CoreType::Constructor(psrs_core::TypeConstructor::String),
        ],
        CoreType::Constructor(psrs_core::TypeConstructor::Unit),
    )
    .expect("option<string> flattens to Int -> String -> Unit");

    assert!(
        validate(
            &import,
            vec![
                CoreType::Constructor(psrs_core::TypeConstructor::Char),
                CoreType::Constructor(psrs_core::TypeConstructor::String)
            ],
            CoreType::Constructor(psrs_core::TypeConstructor::Unit),
        )
        .is_err(),
        "Char is not the option discriminant"
    );
    let mut empty_record_module = psrs_core::Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: Vec::new(),
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: span(),
    };
    empty_record_module.types.push(CoreType::RowEmpty);
    empty_record_module
        .types
        .push(CoreType::Constructor(CoreTypeConstructor::Record));
    empty_record_module
        .types
        .push(CoreType::Application(CoreTypeId(1), CoreTypeId(0)));
    empty_record_module
        .types
        .push(CoreType::Constructor(CoreTypeConstructor::Unit));
    let empty_record_function =
        push_core_arrow(&mut empty_record_module, CoreTypeId(2), CoreTypeId(3));
    assert!(
        crate::abi::link::validate_import_signature(
            &import,
            &empty_record_module,
            empty_record_function
        )
        .is_err()
    );

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
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: Vec::new(),
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: span(),
    };
    // DEC-13 interns a payload-bearing source type such as `Maybe String`,
    // whose conformance is validated against the WIT descriptor later.
    assert!(
        crate::abi::intern_source_type(&mut module, &named).is_some(),
        "a named source type has a resolved source mapping"
    );

    let mut lowerer = RecordingLowerer::default();
    let discriminant = ValueId(30);
    let text = ValueId(31);
    lower(
        &mut lowerer,
        &import,
        &cc_signature(vec![ValueShape::Integer, ValueShape::String]),
        None,
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
    // DEC-13 classifies `option<string>` as `Data.Maybe.Maybe String`.
    assert_eq!(read.canonical_result, Some(option(CanonicalType::String)));
    assert!(read.unsupported.is_none());
    assert!(
        validate(
            &read,
            Vec::new(),
            CoreType::Constructor(psrs_core::TypeConstructor::String)
        )
        .is_err(),
        "a bare String is not the mapped Maybe String"
    );

    let take = registry
        .import("wasi:io/streams", "take")
        .expect("take should resolve");
    assert_eq!(
        take.params,
        vec![option(CanonicalType::Handle {
            resource: crate::abi::canonical::ResourceId {
                interface: "wasi:io/streams@0.2.12".into(),
                name: "output-stream".into(),
            },
            ownership: Ownership::Borrow,
        })]
    );
    assert_eq!(
        flat_leaves_of(&take.params),
        vec![FlatLeaf::Int32, FlatLeaf::Handle]
    );
    validate(
        &take,
        vec![
            CoreType::Constructor(psrs_core::TypeConstructor::Int),
            CoreType::Constructor(psrs_core::TypeConstructor::Int),
        ],
        CoreType::Constructor(psrs_core::TypeConstructor::Unit),
    )
    .expect("a handle payload is declared as Int");
    assert!(
        validate(
            &take,
            vec![
                CoreType::Constructor(psrs_core::TypeConstructor::Char),
                CoreType::Constructor(psrs_core::TypeConstructor::Int)
            ],
            CoreType::Constructor(psrs_core::TypeConstructor::Unit),
        )
        .is_err()
    );
    assert!(
        validate(
            &take,
            vec![
                CoreType::Constructor(psrs_core::TypeConstructor::Int),
                CoreType::Constructor(psrs_core::TypeConstructor::Char)
            ],
            CoreType::Constructor(psrs_core::TypeConstructor::Unit),
        )
        .is_err(),
        "a handle slot is not a Char"
    );
}

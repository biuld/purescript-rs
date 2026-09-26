use super::{SourceSignature, SourceType, WasiRegistry, WasiResultKind};
use crate::abi::{WasiImport, WasiParamKind, WasiResultKind as ResultKind, source_signature};
use crate::types::ValueType;
use psrs_core::{ConstructorInfo, Module as CoreModule, TypeId as CoreTypeId};
use psrs_hir::{
    BuiltinType, ModuleId, SymbolId, Type as HirType, TypeId as HirTypeId, TypeKind as HirTypeKind,
};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn hir_type(kind: HirTypeKind) -> HirType {
    HirType { kind, span: span() }
}

fn empty_core() -> CoreModule {
    CoreModule {
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
    }
}

fn resource(type_id: HirTypeId) -> SourceType {
    SourceType::Resource { type_id }
}

#[test]
fn maps_a_nullary_opaque_type_to_a_wit_resource_handle() {
    let type_id = HirTypeId::new(ModuleId(0), 0);
    let core = empty_core();
    let signature = source_signature(
        &core,
        &hir_type(HirTypeKind::Function {
            parameter: Box::new(hir_type(HirTypeKind::Opaque(type_id))),
            result: Box::new(hir_type(HirTypeKind::Opaque(type_id))),
        }),
    )
    .expect("a nullary opaque type should have a resource ABI mapping");
    assert_eq!(signature.parameters, vec![resource(type_id)]);
    assert_eq!(signature.result, resource(type_id));

    let applied = source_signature(
        &core,
        &hir_type(HirTypeKind::Application(
            Box::new(hir_type(HirTypeKind::Opaque(type_id))),
            Box::new(hir_type(HirTypeKind::Constructor(BuiltinType::Int))),
        )),
    );
    assert!(
        applied.is_none(),
        "an applied foreign constructor is not a resource handle"
    );

    let mut ordinary = empty_core();
    ordinary.constructors.push(ConstructorInfo {
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "Mk".into(),
        type_id,
        tag: 0,
        field_count: 1,
        field_types: vec![CoreTypeId(0)],
    });
    assert!(
        source_signature(&ordinary, &hir_type(HirTypeKind::Named(type_id))).is_none(),
        "an ordinary data type is not a WIT resource"
    );

    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let stdout = registry
        .import("wasi:cli/stdout", "get-stdout")
        .expect("get-stdout should resolve");
    assert_eq!(stdout.result_kind, WasiResultKind::Handle);
    let handle_result = SourceSignature {
        parameters: Vec::new(),
        result: resource(type_id),
        span: span(),
    };
    registry
        .validate_signature(&stdout, &handle_result)
        .expect("get-stdout should accept the opaque output stream");
    registry
        .validate_signature(
            &stdout,
            &SourceSignature {
                result: SourceType::Int,
                ..handle_result
            },
        )
        .expect("the integer placeholder should still match a handle result");
    assert!(
        registry
            .validate_signature(
                &stdout,
                &SourceSignature {
                    parameters: Vec::new(),
                    result: SourceType::Boolean,
                    span: span(),
                },
            )
            .is_err()
    );

    let write = registry
        .import(
            "wasi:io/streams",
            "[method]output-stream.blocking-write-and-flush",
        )
        .expect("blocking-write-and-flush should resolve");
    assert_eq!(
        write.param_kinds,
        vec![WasiParamKind::Handle, WasiParamKind::List]
    );
    let write_signature = SourceSignature {
        parameters: vec![resource(type_id), SourceType::String],
        result: SourceType::Unit,
        span: span(),
    };
    registry
        .validate_signature(&write, &write_signature)
        .expect("the method should accept an opaque resource receiver");
    registry
        .validate_signature(
            &write,
            &SourceSignature {
                parameters: vec![SourceType::Int, SourceType::String],
                ..write_signature.clone()
            },
        )
        .expect("the integer placeholder should still match a handle parameter");
    assert!(
        registry
            .validate_signature(
                &write,
                &SourceSignature {
                    parameters: vec![SourceType::Boolean, SourceType::String],
                    ..write_signature
                },
            )
            .is_err()
    );

    let shape = crate::cc::abstract_signature(
        &SourceSignature {
            parameters: vec![resource(type_id)],
            result: resource(type_id),
            span: span(),
        },
        &core,
        &std::collections::HashMap::new(),
    )
    .expect("a resource should have an abstract integer shape");
    assert_eq!(shape.parameters, vec![crate::cc::ValueShape::Integer]);
    assert_eq!(shape.result, crate::cc::ValueShape::Integer);
}

#[test]
fn does_not_treat_an_opaque_type_as_an_integer_scalar() {
    let type_id = HirTypeId::new(ModuleId(1), 2);
    let import = WasiImport {
        symbol: SymbolId::new(ModuleId(0), 0),
        module: "test:scalar".into(),
        name: "width".into(),
        parameters: vec![ValueType::I32],
        param_kinds: vec![WasiParamKind::Integer32],
        result: Some(ValueType::I32),
        result_kind: ResultKind::Scalar,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    };
    let signature = SourceSignature {
        parameters: vec![resource(type_id)],
        result: resource(type_id),
        span: span(),
    };
    assert!(
        WasiRegistry::load()
            .expect("WASI WIT should load")
            .validate_signature(&import, &signature)
            .is_err(),
        "a resource is not a substitute for a WIT integer"
    );
}

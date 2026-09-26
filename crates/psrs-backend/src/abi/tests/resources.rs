use super::*;
use crate::abi::{WasiImport, WasiParamKind, WasiResultKind as ResultKind};
use crate::types::ValueType;
use psrs_core::{Type as CoreType, TypeConstructor};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn opaque_type(type_id: HirTypeId) -> CoreType {
    CoreType::Constructor(TypeConstructor::User(type_id))
}

#[test]
fn maps_a_nullary_opaque_type_to_a_wit_resource_handle() {
    let type_id = HirTypeId::new(ModuleId(0), 0);
    let mut core = empty_core_module();
    core.opaque_ids.push(type_id);
    let opaque = intern_all(&mut core, vec![opaque_type(type_id)])
        .pop()
        .expect("one opaque type");
    let integer = intern_all(&mut core, vec![CoreType::I32])
        .pop()
        .expect("one integer");
    let boolean = intern_all(&mut core, vec![CoreType::Boolean])
        .pop()
        .expect("one boolean");
    let string = intern_all(&mut core, vec![CoreType::String])
        .pop()
        .expect("one string");
    let unit = unit_type(&mut core);

    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let stdout = registry
        .import("wasi:cli/stdout", "get-stdout")
        .expect("get-stdout should resolve");
    let WasiResultKind::Handle(owned) = &stdout.result_kind else {
        panic!(
            "get-stdout should return an owned handle, got {:?}",
            stdout.result_kind
        );
    };
    assert_eq!(owned.mode, HandleMode::Own);
    assert_eq!(owned.name, "output-stream");
    assert_eq!(owned.interface, "wasi:io/streams@0.2.12");
    assert_eq!(
        registry.symbol_name(owned.drop_symbol),
        Some(("wasi:io/streams@0.2.12", "[resource-drop]output-stream"))
    );
    validate_against(&stdout, core.clone(), &[], opaque)
        .expect("get-stdout should accept the opaque output stream");
    validate_against(&stdout, core.clone(), &[], integer)
        .expect("the integer placeholder should still match a handle result");
    assert!(
        validate_against(&stdout, core.clone(), &[], boolean).is_err(),
        "a Boolean is not a handle result"
    );

    let write = registry
        .import(
            "wasi:io/streams",
            "[method]output-stream.blocking-write-and-flush",
        )
        .expect("blocking-write-and-flush should resolve");
    let WasiParamKind::Handle(borrowed) = &write.param_kinds[0] else {
        panic!(
            "the method receiver should be a borrow, got {:?}",
            write.param_kinds
        );
    };
    assert_eq!(borrowed.mode, HandleMode::Borrow);
    assert_eq!(borrowed.name, "output-stream");
    assert_eq!(write.param_kinds[1], WasiParamKind::List);
    validate_against(&write, core.clone(), &[opaque, string], unit)
        .expect("the method should accept an opaque resource receiver");
    validate_against(&write, core.clone(), &[integer, string], unit)
        .expect("the integer placeholder should still match a handle parameter");
    assert!(
        validate_against(&write, core.clone(), &[boolean, string], unit).is_err(),
        "a Boolean is not a handle parameter"
    );

    let function = psrs_hir::Type {
        kind: psrs_hir::TypeKind::Function {
            parameter: Box::new(psrs_hir::Type {
                kind: psrs_hir::TypeKind::Opaque(type_id),
                span: span(),
            }),
            result: Box::new(psrs_hir::Type {
                kind: psrs_hir::TypeKind::Opaque(type_id),
                span: span(),
            }),
        },
        span: span(),
    };
    let function_id = crate::abi::intern_source_type(&mut core, &function)
        .expect("the resource function type should intern");
    let shape = crate::cc::abstract_signature(
        Some(function_id),
        &core,
        &std::collections::HashMap::new(),
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
    let mut core = empty_core_module();
    core.opaque_ids.push(type_id);
    let opaque = intern_all(&mut core, vec![opaque_type(type_id)])
        .pop()
        .expect("one opaque type");
    assert!(
        validate_against(&import, core, &[opaque], opaque).is_err(),
        "a resource is not a substitute for a WIT integer"
    );
}

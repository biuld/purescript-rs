use super::canonical::CanonicalType;
use super::test_support::import;
use super::*;
use crate::types::ValueType;
use psrs_core::{ConstructorInfo, Type as CoreType, TypeConstructor};
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
    let owned = stdout
        .canonical_result
        .as_ref()
        .and_then(CanonicalType::handle_resource)
        .expect("get-stdout should return an owned handle");
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
    let borrowed = write.params[0]
        .handle_resource()
        .expect("the method receiver should be a borrow");
    assert_eq!(borrowed.mode, HandleMode::Borrow);
    assert_eq!(borrowed.name, "output-stream");
    assert!(write.params[1].is_byte_list());
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
        &std::collections::HashMap::new(),
    )
    .expect("a resource should have an abstract integer shape");
    assert_eq!(shape.parameters, vec![crate::cc::ValueShape::Integer]);
    assert_eq!(shape.result, crate::cc::ValueShape::Integer);
}

#[test]
fn does_not_treat_an_opaque_type_as_an_integer_scalar() {
    let type_id = HirTypeId::new(ModuleId(1), 2);
    let import = import(
        SymbolId::new(ModuleId(0), 0),
        "test:scalar",
        "width",
        vec![CanonicalType::Int {
            width: 32,
            signed: true,
        }],
        Some(CanonicalType::Int {
            width: 32,
            signed: true,
        }),
    );
    let mut core = empty_core_module();
    core.opaque_ids.push(type_id);
    let opaque = intern_all(&mut core, vec![opaque_type(type_id)])
        .pop()
        .expect("one opaque type");
    assert!(
        validate_against(&import, core, &[opaque], opaque).is_err(),
        "a resource is not a substitute for a WIT integer"
    );
    let _ = ValueType::I32;
}

#[test]
fn maps_a_resource_drop_to_the_opaque_handle_parameter() {
    let type_id = HirTypeId::new(ModuleId(0), 7);
    let mut core = empty_core_module();
    core.opaque_ids.push(type_id);
    let opaque = intern_all(&mut core, vec![opaque_type(type_id)])
        .pop()
        .expect("one opaque type");
    let unit = unit_type(&mut core);

    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let drop = registry
        .import("wasi:io/streams", "[resource-drop]input-stream")
        .expect("the input-stream drop should resolve");
    assert_eq!(drop.params.len(), 1);
    let resource = drop.params[0]
        .handle_resource()
        .expect("the drop intrinsic takes a handle");
    assert_eq!(resource.name, "input-stream");
    validate_against(&drop, core.clone(), &[opaque], unit)
        .expect("an opaque input-stream should match the drop handle");
    let integer = intern_all(&mut core, vec![CoreType::I32])
        .pop()
        .expect("one integer");
    validate_against(&drop, core.clone(), &[integer], unit)
        .expect("a bare i32 handle index should match the drop handle");
}

#[test]
fn erases_a_newtype_resource_wrapper_to_its_handle() {
    // `newtype Resource a = Resource Int` applied to an opaque `InputStream`
    // must match the `borrow<input-stream>` parameter by erasing to `Int`.
    let resource_id = HirTypeId::new(ModuleId(0), 0);
    let input_id = HirTypeId::new(ModuleId(0), 1);
    let pollable_id = HirTypeId::new(ModuleId(0), 2);
    let constructor_symbol = SymbolId::new(ModuleId(0), 0);
    let mut core = empty_core_module();
    core.newtype_ids.push(resource_id);
    core.opaque_ids.push(input_id);
    core.opaque_ids.push(pollable_id);
    let input = intern_all(&mut core, vec![opaque_type(input_id)])
        .pop()
        .expect("one opaque input-stream");
    let pollable = intern_all(&mut core, vec![opaque_type(pollable_id)])
        .pop()
        .expect("one opaque pollable");
    let int = intern_all(&mut core, vec![CoreType::I32])
        .pop()
        .expect("one integer");
    core.constructors.push(ConstructorInfo {
        symbol: constructor_symbol,
        name: "Resource".into(),
        type_id: resource_id,
        tag: 0,
        field_count: 1,
        field_types: vec![int],
    });
    let resource_constructor = intern_all(
        &mut core,
        vec![CoreType::Constructor(TypeConstructor::User(resource_id))],
    )
    .pop()
    .expect("one resource constructor");
    let applied = intern_all(
        &mut core,
        vec![CoreType::Application(resource_constructor, input)],
    )
    .pop()
    .expect("one applied resource");

    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let subscribe = registry
        .import("wasi:io/streams", "[method]input-stream.subscribe")
        .expect("input-stream.subscribe should resolve");
    validate_against(&subscribe, core.clone(), &[applied], pollable)
        .expect("Resource InputStream should erase to the input-stream handle");
}

use super::*;
use psrs_core::{Type as CoreType, TypeId as CoreTypeId};
use wit_parser::Type as WitType;

mod capability_gates;
mod enums;
mod indirect;
mod integers;
mod lists;
mod records;
mod resources;

fn empty_core_module() -> psrs_core::Module {
    psrs_core::Module {
        id: psrs_hir::ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: Vec::new(),
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: Vec::new(),
        entry: None,
        span: psrs_span::TextRange::new(0, 0),
    }
}

fn intern_all(module: &mut psrs_core::Module, types: Vec<CoreType>) -> Vec<CoreTypeId> {
    types
        .into_iter()
        .map(|ty| {
            module.types.push(ty);
            CoreTypeId((module.types.len() - 1) as u32)
        })
        .collect()
}

fn function_type(
    module: &mut psrs_core::Module,
    parameters: &[CoreTypeId],
    result: CoreTypeId,
) -> CoreTypeId {
    let mut current = result;
    for parameter in parameters.iter().rev() {
        module.types.push(CoreType::Function {
            parameter: *parameter,
            result: current,
        });
        current = CoreTypeId((module.types.len() - 1) as u32);
    }
    current
}

/// Validates a declaration whose parameters and result are the given Core types
/// against the resolved WIT descriptor, using the production conformance check.
fn validate_core(
    import: &WasiImport,
    parameters: Vec<CoreType>,
    result: CoreType,
) -> Result<(), String> {
    let mut module = empty_core_module();
    let parameter_ids = intern_all(&mut module, parameters);
    let result_id = intern_all(&mut module, vec![result])
        .pop()
        .expect("one result");
    validate_against(import, module, &parameter_ids, result_id)
}

/// Validates against a prepared module with the given parameter and result ids.
fn validate_against(
    import: &WasiImport,
    module: psrs_core::Module,
    parameters: &[CoreTypeId],
    result: CoreTypeId,
) -> Result<(), String> {
    let mut module = module;
    let function = function_type(&mut module, parameters, result);
    crate::abi::link::validate_import_signature(import, &module, function)
}

/// A module holding a closed record with the given fields, and the record id.
fn record_module(fields: &[(&str, CoreType)]) -> (psrs_core::Module, CoreTypeId) {
    let mut module = empty_core_module();
    let mut ids = Vec::with_capacity(fields.len());
    for (label, ty) in fields {
        let id = intern_all(&mut module, vec![ty.clone()])
            .pop()
            .expect("one field type");
        ids.push(((*label).to_string(), id));
    }
    let record = intern_all(&mut module, vec![CoreType::Record(ids)])
        .pop()
        .expect("one record");
    (module, record)
}

fn unit_type(module: &mut psrs_core::Module) -> CoreTypeId {
    intern_all(module, vec![CoreType::Unit])
        .pop()
        .expect("one unit")
}

#[test]
fn resolves_stdout_and_exit_imports() {
    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let stdout = registry
        .import(names::STDOUT, names::GET_STDOUT)
        .expect("get-stdout should resolve");
    assert_eq!(stdout.module, "wasi:cli/stdout@0.2.12");
    assert!(stdout.parameters.is_empty());
    assert!(stdout.param_kinds.is_empty());
    assert_eq!(stdout.result, Some(ValueType::I32));

    let write = registry
        .import(names::STREAMS, names::WRITE_STDOUT)
        .expect("blocking-write-and-flush should resolve");
    assert_eq!(write.module, "wasi:io/streams@0.2.12");
    let super::WasiParamKind::Handle(receiver) = &write.param_kinds[0] else {
        panic!(
            "the stream receiver should be a handle, got {:?}",
            write.param_kinds
        );
    };
    assert_eq!(receiver.mode, super::HandleMode::Borrow);
    assert_eq!(receiver.name, "output-stream");
    assert_eq!(write.param_kinds[1], WasiParamKind::List);
    assert_eq!(write.result_kind, WasiResultKind::Result);
    assert!(write.retptr);
    assert!(write.unsupported.is_none());

    let read = registry
        .import("wasi:io/streams", "[method]input-stream.read")
        .expect("input-stream.read should resolve");
    assert!(
        read.unsupported
            .as_deref()
            .is_some_and(|message| { message.contains("payload on success") })
    );
    let exit = registry
        .import(names::EXIT, names::EXIT_WITH_CODE)
        .expect("exit-with-code should resolve");
    assert_eq!(exit.module, "wasi:cli/exit@0.2.12");
    assert_eq!(exit.parameters, vec![ValueType::I32]);
    assert_eq!(exit.result, None);

    // Interning returns the same symbol for the same import.
    let stdout_again = registry
        .import(names::STDOUT, names::GET_STDOUT)
        .expect("get-stdout should resolve again");
    assert_eq!(stdout.symbol, stdout_again.symbol);
}

#[test]
fn classifies_a_64_bit_parameter_by_its_wit_signedness() {
    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let random = registry
        .import("wasi:random/random", "get-random-bytes")
        .expect("get-random-bytes should resolve");
    assert_eq!(
        random.param_kinds,
        vec![WasiParamKind::Scalar64 { signed: false }]
    );
}

#[test]
fn maps_wit_char_to_the_source_char_type() {
    assert_eq!(
        param_kind(&Resolve::default(), &WitType::Char),
        WasiParamKind::Char
    );
    assert_eq!(
        result_kind(&Resolve::default(), &WitType::Char),
        WasiResultKind::Char
    );

    let import = WasiImport {
        symbol: psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
        module: "test:chars".into(),
        name: "roundtrip".into(),
        parameters: vec![ValueType::I32],
        param_kinds: vec![WasiParamKind::Char],
        result: Some(ValueType::I32),
        result_kind: WasiResultKind::Char,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    };
    validate_core(&import, vec![CoreType::Char], CoreType::Char)
        .expect("a Char declaration should match WIT char");
    assert!(
        validate_core(&import, vec![CoreType::I32], CoreType::I32).is_err(),
        "an Int is not the source Char type"
    );
}

#[test]
fn classifies_wit_f32_for_number_conversion() {
    assert_eq!(
        param_kind(&Resolve::default(), &WitType::F32),
        WasiParamKind::Float32
    );
    let import = WasiImport {
        symbol: psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
        module: "test:floats".into(),
        name: "roundtrip".into(),
        parameters: vec![ValueType::F32],
        param_kinds: vec![WasiParamKind::Float32],
        result: Some(ValueType::F32),
        result_kind: WasiResultKind::Scalar,
        unsupported: None,
        retptr: false,
        flat_slots: Vec::new(),
    };
    validate_core(&import, vec![CoreType::F64], CoreType::F64)
        .expect("source Number should adapt to and from WIT f32");
}

#[test]
fn classifies_only_source_compatible_wit_scalar_parameters() {
    assert_eq!(
        param_kind(&Resolve::default(), &WitType::Bool),
        WasiParamKind::Boolean
    );
    assert_eq!(
        param_kind(&Resolve::default(), &WitType::S32),
        WasiParamKind::Integer32
    );
    assert_eq!(
        param_kind(&Resolve::default(), &WitType::F64),
        WasiParamKind::Float64
    );
    assert_eq!(
        param_kind(&Resolve::default(), &WitType::U32),
        WasiParamKind::Integer32
    );
    assert_eq!(
        result_kind(&Resolve::default(), &WitType::Bool),
        WasiResultKind::Boolean
    );
    assert_eq!(
        result_kind(&Resolve::default(), &WitType::U32),
        WasiResultKind::Scalar
    );
}

#[test]
fn validates_wit_scalar_parameters_against_exact_source_types() {
    let cases = [
        (WasiParamKind::Integer32, CoreType::I32, CoreType::Boolean),
        (WasiParamKind::Boolean, CoreType::Boolean, CoreType::I32),
        (WasiParamKind::Float64, CoreType::F64, CoreType::I32),
    ];
    for (index, (kind, accepted, rejected)) in cases.into_iter().enumerate() {
        let import = WasiImport {
            symbol: psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), index as u32),
            module: "test:scalar".into(),
            name: "accepts-one-value".into(),
            parameters: vec![match kind {
                WasiParamKind::Integer32 | WasiParamKind::Boolean => ValueType::I32,
                WasiParamKind::Float64 => ValueType::F64,
                _ => unreachable!("this test only covers scalar parameters"),
            }],
            param_kinds: vec![kind],
            result: None,
            result_kind: WasiResultKind::None,
            unsupported: None,
            retptr: false,
            flat_slots: Vec::new(),
        };
        validate_core(&import, vec![accepted], CoreType::Unit)
            .expect("the matching source scalar should be accepted");
        assert!(
            validate_core(&import, vec![rejected], CoreType::Unit).is_err(),
            "an incompatible source scalar must be rejected"
        );
    }
}

#[test]
fn validates_a_vendored_wit_boolean_result_against_boolean_source_type() {
    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let import = registry
        .import("wasi:io/poll", "[method]pollable.ready")
        .expect("pollable.ready should resolve");
    assert_eq!(import.result_kind, WasiResultKind::Boolean);
    validate_core(&import, vec![CoreType::I32], CoreType::Boolean)
        .expect("pollable.ready should accept its Boolean source signature");
    assert!(validate_core(&import, vec![CoreType::I32], CoreType::I32).is_err());
}

#[test]
fn rejects_a_disabled_wasi_service_before_lowering() {
    let target = TargetCapabilities {
        wasi_random: false,
        ..TargetCapabilities::default()
    };
    let mut registry = WasiRegistry::load_with_capabilities(target)
        .expect("WASI WIT should load with a restricted target");
    let random = registry
        .import("wasi:random/random", "get-random-bytes")
        .expect("the WIT declaration should still resolve");
    assert!(
        random
            .unsupported
            .as_deref()
            .is_some_and(|message| message.contains("disabled"))
    );
}

use super::canonical::{CanonicalType, resolve as canonical_resolve};
use super::test_support::import;
use super::*;
use psrs_core::{Type as CoreType, TypeConstructor as CoreTypeConstructor, TypeId as CoreTypeId};
use wit_parser::Type as WitType;

mod aggregates;
mod capability_gates;
mod enums;
mod indirect;
mod integers;
mod lists;
mod records;
mod resources;

fn empty_core_module() -> psrs_core::Module {
    psrs_core::Module {
        type_names: Vec::new(),
        id: psrs_hir::ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types: Vec::new(),
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
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
        current = push_core_arrow(module, *parameter, current);
    }
    current
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
    let record = push_record(&mut module, ids);
    (module, record)
}

/// Appends a closed record as `Application(Constructor(Record), row)` and
/// returns its id.
fn push_record(module: &mut psrs_core::Module, fields: Vec<(String, CoreTypeId)>) -> CoreTypeId {
    let mut fields = fields;
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    let row_empty = intern_all(module, vec![CoreType::RowEmpty])
        .pop()
        .expect("one row empty");
    let mut tail = row_empty;
    for (label, ty) in fields.into_iter().rev() {
        tail = intern_all(module, vec![CoreType::RowExtend { label, ty, tail }])
            .pop()
            .expect("one row extend");
    }
    let head = intern_all(
        module,
        vec![CoreType::Constructor(CoreTypeConstructor::Record)],
    )
    .pop()
    .expect("one record head");
    intern_all(module, vec![CoreType::Application(head, tail)])
        .pop()
        .expect("one record")
}

fn unit_type(module: &mut psrs_core::Module) -> CoreTypeId {
    intern_all(
        module,
        vec![CoreType::Constructor(psrs_core::TypeConstructor::Unit)],
    )
    .pop()
    .expect("one unit")
}

fn int(width: u8, signed: bool) -> CanonicalType {
    CanonicalType::Int { width, signed }
}

/// The source-ABI surface reason for a resolved WIT function, if any.
fn unsupported(resolve: &Resolve, function: &wit_parser::Function) -> Option<String> {
    let params = function
        .params
        .iter()
        .map(|param| canonical_resolve(resolve, &param.ty))
        .collect::<Vec<_>>();
    let result = function
        .result
        .as_ref()
        .and_then(|ty| canonical_resolve(resolve, ty));
    let result_shape = match &function.result {
        None => Ok(None),
        Some(_) => result.clone().map(Some).ok_or(()),
    };
    super::validation::unsupported_shape(&params, &result_shape, &result)
}

#[test]
fn resolves_stdout_and_exit_imports() {
    let mut registry = WasiRegistry::load().expect("WASI WIT should load");
    let stdout = registry
        .import(names::STDOUT, names::GET_STDOUT)
        .expect("get-stdout should resolve");
    assert_eq!(stdout.module, "wasi:cli/stdout@0.2.12");
    assert!(stdout.parameters.is_empty());
    assert!(stdout.params.is_empty());
    assert_eq!(stdout.result, Some(ValueType::I32));

    let write = registry
        .import(names::STREAMS, names::WRITE_STDOUT)
        .expect("blocking-write-and-flush should resolve");
    assert_eq!(write.module, "wasi:io/streams@0.2.12");
    let receiver = write.params[0]
        .handle_resource()
        .expect("the stream receiver should be a handle");
    assert_eq!(receiver.mode, HandleMode::Borrow);
    assert_eq!(receiver.name, "output-stream");
    // DEC-16: `list<u8>` is `Array Int`, not `String`: its bytes are
    // uninterpreted and stay out of the Unicode text type.
    assert_eq!(
        write.params[1],
        CanonicalType::List(Box::new(CanonicalType::Int {
            width: 8,
            signed: false
        }))
    );
    assert!(matches!(
        write.canonical_result,
        Some(CanonicalType::Result { ok: None, .. })
    ));
    assert!(write.abi.retptr);
    assert!(write.unsupported.is_none());

    let read = registry
        .import("wasi:io/streams", "[method]input-stream.read")
        .expect("input-stream.read should resolve");
    // DEC-13 maps `result<list<u8>, stream-error>` to
    // `Either StreamError (Array Int)`.
    assert!(matches!(
        read.canonical_result,
        Some(CanonicalType::Result {
            ok: Some(_),
            err: Some(_)
        })
    ));
    assert!(read.unsupported.is_none());
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
    assert_eq!(random.params, vec![int(64, false)]);
}

#[test]
fn maps_wit_char_to_the_source_char_type() {
    assert_eq!(
        canonical_resolve(&Resolve::default(), &WitType::Char),
        Some(CanonicalType::Char)
    );

    let import = import(
        psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
        "test:chars",
        "roundtrip",
        vec![CanonicalType::Char],
        Some(CanonicalType::Char),
    );
    validate_core(
        &import,
        vec![CoreType::Constructor(psrs_core::TypeConstructor::Char)],
        CoreType::Constructor(psrs_core::TypeConstructor::Char),
    )
    .expect("a Char declaration should match WIT char");
    assert!(
        validate_core(
            &import,
            vec![CoreType::Constructor(psrs_core::TypeConstructor::Int)],
            CoreType::Constructor(psrs_core::TypeConstructor::Int)
        )
        .is_err(),
        "an Int is not the source Char type"
    );
}

#[test]
fn classifies_wit_f32_for_number_conversion() {
    assert_eq!(
        canonical_resolve(&Resolve::default(), &WitType::F32),
        Some(CanonicalType::Float { width: 32 })
    );
    let import = import(
        psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
        "test:floats",
        "roundtrip",
        vec![CanonicalType::Float { width: 32 }],
        Some(CanonicalType::Float { width: 32 }),
    );
    validate_core(
        &import,
        vec![CoreType::Constructor(psrs_core::TypeConstructor::Number)],
        CoreType::Constructor(psrs_core::TypeConstructor::Number),
    )
    .expect("source Number should adapt to and from WIT f32");
}

#[test]
fn classifies_only_source_compatible_wit_scalar_parameters() {
    let resolve = Resolve::default();
    assert_eq!(
        canonical_resolve(&resolve, &WitType::Bool),
        Some(CanonicalType::Bool)
    );
    assert_eq!(
        canonical_resolve(&resolve, &WitType::S32),
        Some(int(32, true))
    );
    assert_eq!(
        canonical_resolve(&resolve, &WitType::F64),
        Some(CanonicalType::Float { width: 64 })
    );
    assert_eq!(
        canonical_resolve(&resolve, &WitType::U32),
        Some(int(32, false))
    );
}

#[test]
fn validates_wit_scalar_parameters_against_exact_source_types() {
    let cases = [
        (
            int(32, false),
            CoreType::Constructor(psrs_core::TypeConstructor::Int),
            CoreType::Constructor(psrs_core::TypeConstructor::Boolean),
        ),
        (
            CanonicalType::Bool,
            CoreType::Constructor(psrs_core::TypeConstructor::Boolean),
            CoreType::Constructor(psrs_core::TypeConstructor::Int),
        ),
        (
            CanonicalType::Float { width: 64 },
            CoreType::Constructor(psrs_core::TypeConstructor::Number),
            CoreType::Constructor(psrs_core::TypeConstructor::Int),
        ),
    ];
    for (index, (ty, accepted, rejected)) in cases.into_iter().enumerate() {
        let import = import(
            psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), index as u32),
            "test:scalar",
            "accepts-one-value",
            vec![ty],
            None,
        );
        validate_core(
            &import,
            vec![accepted],
            CoreType::Constructor(psrs_core::TypeConstructor::Unit),
        )
        .expect("the matching source scalar should be accepted");
        assert!(
            validate_core(
                &import,
                vec![rejected],
                CoreType::Constructor(psrs_core::TypeConstructor::Unit)
            )
            .is_err(),
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
    assert_eq!(import.canonical_result, Some(CanonicalType::Bool));
    validate_core(
        &import,
        vec![CoreType::Constructor(psrs_core::TypeConstructor::Int)],
        CoreType::Constructor(psrs_core::TypeConstructor::Boolean),
    )
    .expect("pollable.ready should accept its Boolean source signature");
    assert!(
        validate_core(
            &import,
            vec![CoreType::Constructor(psrs_core::TypeConstructor::Int)],
            CoreType::Constructor(psrs_core::TypeConstructor::Int)
        )
        .is_err()
    );
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

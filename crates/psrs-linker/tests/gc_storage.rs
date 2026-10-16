//! Checked GC provider selection and application composition, without compiler IR.
use psrs_linker::{
    BindingRequirement, Boundary, CoreTypes, Provider, RequirementId, TargetLinkInput, TargetPolicy,
};

fn application(nullable: bool) -> Vec<u8> {
    application_with_expected(nullable, 78)
}

fn application_with_expected(nullable: bool, expected: i32) -> Vec<u8> {
    let array = if nullable {
        "(ref null $a)"
    } else {
        "(ref $a)"
    };
    wat::parse_str(format!(r#"(module
        (type (struct (field i64)))
        (type $a (array (mut eqref)))
        (import "psrs:runtime-storage" "array_fill" (func $fill (param i32 eqref) (result (ref $a))))
        (import "psrs:runtime-storage" "array_write" (func $write (param {array} i32 eqref)))
        (import "psrs:runtime-storage" "array_read" (func $read (param (ref $a) i32) (result eqref)))
        (memory (export "memory") 2)
        (func (export "wasi:cli/run@0.2.12#run") (result i32) (local $a (ref null $a)) (local $alias (ref null $a))
          i32.const 1 i32.const 7 ref.i31 call $fill local.tee $a local.set $alias
          local.get $a ref.as_non_null i32.const 0 i32.const 78 ref.i31 call $write
          local.get $alias ref.as_non_null i32.const 0 call $read
          ref.cast (ref i31) i31.get_s i32.const {expected} i32.ne
          local.get $a local.get $alias ref.eq i32.eqz i32.or))"#)).unwrap()
}

fn setup() -> (
    psrs_linker::ResolvedWorldContext,
    psrs_linker::CheckedLinkPlan,
) {
    setup_in_context(psrs_linker::resolve_default_definitions().unwrap())
}

fn setup_in_context(
    context: psrs_linker::ResolvedWorldContext,
) -> (
    psrs_linker::ResolvedWorldContext,
    psrs_linker::CheckedLinkPlan,
) {
    let units = psrs_linker::runtime::package_offers(&psrs_runtime::PSRS_RUNTIME).unwrap();
    let storage = units
        .iter()
        .find(|unit| unit.id == psrs_runtime::STORAGE_UNIT.id)
        .unwrap();
    let requirements = storage
        .provided
        .iter()
        .filter(|op| op.export != "trap")
        .enumerate()
        .map(|(index, op)| BindingRequirement {
            id: RequirementId(index as u32),
            origin: op.name.clone(),
            boundary: Boundary::RawCore {
                module: psrs_runtime::STORAGE_MODULE.into(),
                field: op.export.clone(),
            },
            expected: Some(op.signature.clone()),
            provider: Provider::RuntimeOperation {
                name: op.name.clone(),
                version: op.version.clone(),
            },
        })
        .collect();
    let plan = psrs_linker::plan(
        &context,
        TargetLinkInput {
            requirements,
            units,
            policy: TargetPolicy::default(),
            memory: psrs_linker::MemoryDemand {
                canonical_scratch: (0, 16),
                allocator_state: (16, 24),
                base_heap_start: 24,
                heap_alignment: 8,
                growth_owner: psrs_linker::GENERATED_GROWTH_OWNER.into(),
                maximum_pages: None,
            },
        },
    )
    .unwrap();
    (context, plan)
}

#[test]
fn checked_provider_selection_matches_gc_contracts_and_rejects_application_drift() {
    let (context, plan) = setup();
    assert_eq!(plan.artifacts().len(), 1);
    let types = CoreTypes::from_module(&application(false)).unwrap();
    let units = psrs_linker::runtime::package_offers(&psrs_runtime::PSRS_RUNTIME).unwrap();
    let storage = units
        .iter()
        .find(|unit| unit.id == psrs_runtime::STORAGE_UNIT.id)
        .unwrap();
    for (index, export) in [(2, "array_fill"), (3, "array_write"), (4, "array_read")] {
        let operation = storage
            .provided
            .iter()
            .find(|op| op.export == export)
            .unwrap();
        assert_eq!(types.signature(index).unwrap(), operation.signature);
    }
    let error = psrs_linker::compose(&context, &plan, &application(true)).unwrap_err();
    assert!(
        error.to_string().contains("application import signature"),
        "{error}"
    );
    let error = match psrs_linker::assemble_core(&plan, &application(true)) {
        Ok(_) => panic!("nullable array drift must fail before direct assembly"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("application import signature"),
        "{error}"
    );
}

#[test]
fn checked_storage_provider_closes_gc_imports() {
    let (context, plan) = setup();
    let app = application(false);
    let linked = psrs_linker::compose(&context, &plan, &app).unwrap();
    let units = psrs_linker::runtime::package_offers(&psrs_runtime::PSRS_RUNTIME).unwrap();
    let provider = &units
        .iter()
        .find(|unit| unit.id == psrs_runtime::STORAGE_UNIT.id)
        .unwrap()
        .artifact
        .bytes;
    let mut originals = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&linked.bytes) {
        if let wasmparser::Payload::ModuleSection {
            unchecked_range, ..
        } = payload.unwrap()
        {
            let bytes = &linked.bytes[unchecked_range];
            if bytes == app || bytes == provider {
                originals += 1;
            }
        }
    }
    assert_eq!(
        originals, 2,
        "the component must retain both original core modules"
    );
    assert!(
        !linked
            .external_world
            .iter()
            .any(|name| name == psrs_runtime::STORAGE_MODULE)
    );
    let wrong =
        psrs_linker::compose(&context, &plan, &application_with_expected(false, 79)).unwrap();
    execute_commands("world", [(linked.bytes, true), (wrong.bytes, false)]);
}

fn direct_command(plan: &psrs_linker::CheckedLinkPlan, app: &[u8]) -> Vec<u8> {
    use wasm_encoder::{ComponentExportKind, ComponentValType, ExportKind};
    let assembled = psrs_linker::assemble_core(plan, app).unwrap();
    let application = assembled.application_instance();
    let mut component = assembled.into_encoder();
    let run = component.core_alias_export(
        None,
        application,
        "wasi:cli/run@0.2.12#run",
        ExportKind::Func,
    );
    let (result, ty) = component.type_defined(None);
    ty.result(None, None);
    let (signature, mut ty) = component.type_function(None);
    ty.params(std::iter::empty::<(&str, ComponentValType)>());
    ty.result(Some(ComponentValType::Type(result)));
    let run = component.lift_func(None, run, signature, []);
    let instance = component.instance_count();
    let mut bytes = component.finish();
    use wasm_encoder::ComponentSection;
    let mut instances = wasm_encoder::ComponentInstanceSection::new();
    instances.export_items([("run", ComponentExportKind::Func, run)]);
    instances.append_to_component(&mut bytes);
    let mut exports = wasm_encoder::ComponentExportSection::new();
    exports.export(
        "wasi:cli/run@0.2.12",
        ComponentExportKind::Instance,
        instance,
        None,
    );
    exports.append_to_component(&mut bytes);
    wasmparser::Validator::new().validate_all(&bytes).unwrap();
    bytes
}

#[test]
fn direct_core_instance_assembly_executes_gc_storage_inside_a_component() {
    let (_, plan) = setup();
    execute_commands(
        "direct",
        [
            (direct_command(&plan, &application(false)), true),
            (
                direct_command(&plan, &application_with_expected(false, 79)),
                false,
            ),
        ],
    );
}

fn execute_commands(label: &str, cases: [(Vec<u8>, bool); 2]) {
    let version = std::process::Command::new("wasmtime")
        .arg("--version")
        .output();
    if !version.is_ok_and(|output| output.status.success()) {
        assert_ne!(
            std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(),
            Ok("1"),
            "required Wasmtime unavailable"
        );
        eprintln!("skipping GC component execution: Wasmtime unavailable");
        return;
    }
    let path = std::env::temp_dir().join(format!(
        "psrs-gc-component-{label}-{}.wasm",
        std::process::id()
    ));
    for (bytes, success) in cases {
        std::fs::write(&path, bytes).unwrap();
        let output = std::process::Command::new("wasmtime")
            .arg("run")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "expected success={success}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn canonical_boundary_uses_the_selected_world_and_public_parameters() {
    let mut resolve = wit_parser::Resolve::default();
    let package = resolve
        .push_str(
            "counter.wit",
            "package test:counter; world counter { export check: func(offset: u32) -> u32; }",
        )
        .unwrap();
    let world = resolve.packages[package].worlds["counter"];
    let context = psrs_linker::ResolvedWorldContext::from_resolve(resolve, world);
    let (context, plan) = setup_in_context(context);
    let text = r#"(module
        (type $a (array (mut eqref)))
        (import "psrs:runtime-storage" "array_fill" (func $fill (param i32 eqref) (result (ref $a))))
        (import "psrs:runtime-storage" "array_write" (func $write (param (ref $a) i32 eqref)))
        (import "psrs:runtime-storage" "array_read" (func $read (param (ref $a) i32) (result eqref)))
        (memory (export "memory") 2)
        (func (export "check") (param $offset i32) (result i32) (local $a (ref null $a))
          i32.const 1 ref.null eq call $fill local.set $a
          local.get $a ref.as_non_null i32.const 0 local.get $offset i32.const 1 i32.add ref.i31 call $write
          local.get $a ref.as_non_null i32.const 0 call $read ref.cast (ref i31) i31.get_s))"#;
    let app = wat::parse_str(text).unwrap();
    let linked = psrs_linker::compose(&context, &plan, &app).unwrap();
    assert!(linked.external_world.is_empty());
    let decoded = wit_component::decode(&linked.bytes).unwrap();
    let wit_component::DecodedWasm::Component(resolve, world) = decoded else {
        panic!("expected executable component")
    };
    let wit_parser::WorldItem::Function(function) =
        &resolve.worlds[world].exports[&wit_parser::WorldKey::Name("check".into())]
    else {
        panic!("expected check function")
    };
    assert_eq!(
        function
            .params
            .iter()
            .map(|parameter| (parameter.name.as_str(), parameter.ty))
            .collect::<Vec<_>>(),
        vec![("offset", wit_parser::Type::U32)]
    );
    assert_eq!(function.result, Some(wit_parser::Type::U32));
    let wrong = text
        .replace("(param $offset i32)", "(param $offset i64)")
        .replace("local.get $offset", "local.get $offset i32.wrap_i64");
    let wrong = wat::parse_str(wrong).unwrap();
    wasmparser::Validator::new().validate_all(&wrong).unwrap();
    let error = psrs_linker::compose(&context, &plan, &wrong).unwrap_err();
    assert!(error.to_string().contains("type mismatch"), "{error}");
}

//! Target-only composition tests: the core module is built with `wasm-encoder`
//! and no compiler IR participates.

use psrs_linker::{
    ArtifactReference, BindingRequirement, Boundary, CoreSignature, CoreType, MemoryDemand,
    Provider, RequirementId, TargetLinkInput, TargetPolicy, plan, resolve_default_definitions,
};
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction, MemorySection, MemoryType, Module, TypeSection, ValType,
};

fn core_module() -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    types
        .ty()
        .function([ValType::F64, ValType::I32, ValType::I32], [ValType::I32]);
    module.section(&types);
    let mut imports = ImportSection::new();
    imports.import(
        psrs_runtime::MODULE_NAME,
        psrs_runtime::NUMBER_EXPORT,
        EntityType::Function(1),
    );
    module.section(&imports);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 4,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("wasi:cli/run@0.2.12#run", ExportKind::Func, 1);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut body = Function::new([]);
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::End);
    code.function(&body);
    module.section(&code);
    module.finish()
}

fn input() -> TargetLinkInput {
    TargetLinkInput {
        requirements: vec![BindingRequirement {
            id: RequirementId(0),
            origin: "NumberToString".into(),
            boundary: Boundary::RawCore {
                module: psrs_runtime::MODULE_NAME.into(),
                field: psrs_runtime::NUMBER_EXPORT.into(),
            },
            expected: Some(CoreSignature {
                parameters: vec![CoreType::F64, CoreType::I32, CoreType::I32],
                result: Some(CoreType::I32),
            }),
            provider: Provider::ArtifactExport {
                artifact: psrs_runtime::NUMBER_RUNTIME.id.into(),
                export: psrs_runtime::NUMBER_EXPORT.into(),
                signature: CoreSignature {
                    parameters: vec![CoreType::F64, CoreType::I32, CoreType::I32],
                    result: Some(CoreType::I32),
                },
            },
        }],
        artifacts: vec![ArtifactReference {
            contract: psrs_linker::runtime::contract(&psrs_runtime::NUMBER_RUNTIME),
            bytes: psrs_runtime::NUMBER_RUNTIME.bytes.to_vec(),
        }],
        policy: TargetPolicy::default(),
        memory: MemoryDemand {
            canonical_scratch: (0, 16),
            allocator_state: (16, 24),
            base_heap_start: 24,
            heap_alignment: 8,
        },
    }
}

#[test]
fn composition_attaches_the_library_and_closes_the_private_import() {
    let context = resolve_default_definitions().unwrap();
    let link = plan(&context, input()).expect("the plan should be valid");
    let core = core_module();
    let composed =
        psrs_linker::compose(&context, &link, &core).expect("composition should succeed");
    wasmparser::Validator::new()
        .validate_all(&composed.bytes)
        .expect("the composed component should validate");
    assert!(
        !composed
            .external_world
            .iter()
            .any(|id| id == psrs_runtime::MODULE_NAME),
        "the private runtime import must be closed: {:?}",
        composed.external_world
    );
}

#[test]
fn composition_without_the_artifact_fails_closed() {
    let context = resolve_default_definitions().unwrap();
    // A plan with no provider cannot close the core module's runtime import.
    let link = plan(
        &context,
        TargetLinkInput {
            requirements: Vec::new(),
            artifacts: Vec::new(),
            policy: TargetPolicy::default(),
            memory: MemoryDemand {
                canonical_scratch: (0, 16),
                allocator_state: (16, 24),
                base_heap_start: 24,
                heap_alignment: 8,
            },
        },
    )
    .unwrap();
    assert!(psrs_linker::compose(&context, &link, &core_module()).is_err());
}

#[test]
fn composition_rejects_a_different_resolved_context() {
    let context = resolve_default_definitions().unwrap();
    let link = plan(&context, input()).unwrap();
    let other = resolve_default_definitions().unwrap();
    let error = psrs_linker::compose(&other, &link, &core_module()).unwrap_err();
    assert!(error.to_string().contains("context differs"));
}

#[test]
fn composition_rejects_memory_and_signature_drift_from_the_plan() {
    let context = resolve_default_definitions().unwrap();
    let link = plan(&context, input()).unwrap();
    let core = core_module();
    // Structural rewrites preserve module validity while violating the plan.
    let text = wasmprinter::print_bytes(&core).unwrap();
    let small = wat::parse_str(text.replace("(memory (;0;) 4)", "(memory (;0;) 1)")).unwrap();
    assert_ne!(small, core);
    assert!(
        psrs_linker::compose(&context, &link, &small)
            .unwrap_err()
            .to_string()
            .contains("memory")
    );
    let changed =
        wat::parse_str(text.replace("(param f64 i32 i32)", "(param f32 i32 i32)")).unwrap();
    assert_ne!(changed, core);
    assert!(
        psrs_linker::compose(&context, &link, &changed)
            .unwrap_err()
            .to_string()
            .contains("signature")
    );
}

#[test]
fn a_host_resource_return_uses_the_executable_interface_closure() {
    let context = resolve_default_definitions().unwrap();
    let mut input = input();
    input.artifacts.clear();
    input.requirements = vec![BindingRequirement {
        id: RequirementId(0),
        origin: "get-stdout".into(),
        boundary: Boundary::ResolvedWit {
            interface: "wasi:cli/stdout@0.2.12".into(),
            function: "get-stdout".into(),
        },
        expected: Some(CoreSignature {
            parameters: vec![],
            result: Some(CoreType::I32),
        }),
        provider: Provider::HostInterface {
            interface: "wasi:cli/stdout@0.2.12".into(),
        },
    }];
    input.policy.permitted_host_interfaces = context.world_imports().to_vec();
    let link = plan(&context, input).unwrap();
    let core = wat::parse_str(
        r#"(module
        (import "wasi:cli/stdout@0.2.12" "get-stdout" (func (result i32)))
        (memory (export "memory") 2)
        (func (export "wasi:cli/run@0.2.12#run") (result i32) call 0 drop i32.const 0))"#,
    )
    .unwrap();
    let linked = psrs_linker::compose(&context, &link, &core).unwrap();
    assert_eq!(linked.external_world, link.external_world());
}

#[test]
fn composition_rejects_unowned_data_and_a_wrong_allocator_boundary() {
    let context = resolve_default_definitions().unwrap();
    let link = plan(&context, input()).unwrap();
    let text = wasmprinter::print_bytes(core_module()).unwrap();
    let prefix = text.trim_end().strip_suffix(')').unwrap();
    for segment in [
        r#"(data (i32.const 131072) "x")"#,
        r#"(data (i32.const 16) "\00\00\00\00\00\00\00\00")"#,
    ] {
        let bytes = wat::parse_str(format!("{prefix}{segment})")).unwrap();
        let error = psrs_linker::compose(&context, &link, &bytes).unwrap_err();
        assert!(error.to_string().contains("application"), "{error}");
    }
}

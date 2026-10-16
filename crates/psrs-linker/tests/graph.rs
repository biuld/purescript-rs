//! Provider-graph closure. These tests build target records only.

use psrs_linker::{
    ArtifactContract, ArtifactKind, ArtifactReference, BindingRequirement, Boundary, CoreSignature,
    CoreType, DeclaredExport, ExportKind, InitializationContract, InitializationStep, MemoryDemand,
    Provider, ProviderEdge, RequiredOperation, RequirementId, RuntimeUnitOffer, TargetLinkInput,
    TargetPolicy, plan, resolve_default_definitions,
};
use wasm_encoder::{
    CodeSection, ExportKind as EncodedExport, ExportSection, Function, FunctionSection,
    Instruction, Module, TypeSection, ValType,
};

fn memory() -> MemoryDemand {
    MemoryDemand {
        canonical_scratch: (0, 16),
        allocator_state: (16, 24),
        base_heap_start: 24,
        heap_alignment: 8,
        growth_owner: psrs_linker::GENERATED_GROWTH_OWNER.into(),
        maximum_pages: None,
    }
}

fn input(requirements: Vec<BindingRequirement>, units: Vec<RuntimeUnitOffer>) -> TargetLinkInput {
    let permitted = resolve_default_definitions()
        .map(|context| context.world_imports().to_vec())
        .unwrap_or_default();
    TargetLinkInput {
        requirements,
        units,
        policy: TargetPolicy {
            permitted_host_interfaces: permitted,
        },
        memory: memory(),
    }
}

fn signature() -> CoreSignature {
    CoreSignature {
        parameters: Vec::new(),
        result: Some(CoreType::I32),
    }
}

fn dependency_bytes() -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([], [ValType::I32]);
    module.section(&types);
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    let mut exports = ExportSection::new();
    exports.export("dep", EncodedExport::Func, 0);
    module.section(&exports);
    let mut code = CodeSection::new();
    let mut body = Function::new([]);
    body.instruction(&Instruction::I32Const(0));
    body.instruction(&Instruction::End);
    code.function(&body);
    module.section(&code);
    module.finish()
}

fn dependency_unit(id: &str, operation: &str) -> RuntimeUnitOffer {
    let bytes = dependency_bytes();
    let digest = psrs_linker::sha256_hex(&bytes);
    RuntimeUnitOffer {
        id: id.into(),
        semantic_contract_version: "1".into(),
        provided: vec![psrs_linker::OfferedOperation {
            name: operation.into(),
            version: "1".into(),
            export: "dep".into(),
            signature: signature(),
        }],
        required: Vec::new(),
        artifact: ArtifactReference {
            contract: ArtifactContract {
                id: format!("{id}-artifact"),
                kind: ArtifactKind::CoreModule,
                module_name: id.into(),
                sha256: digest,
                provenance: "test".into(),
                required_features: Vec::new(),
                imports: Vec::new(),
                exports: vec![DeclaredExport {
                    name: "dep".into(),
                    kind: ExportKind::Func,
                    signature: Some(signature()),
                }],
                tables: Vec::new(),
                elements: Vec::new(),
                globals: Vec::new(),
                storage: None,
                initialization: InitializationContract {
                    start_forbidden: true,
                    data_range: (0, 0),
                },
                instantiate_after_shims: false,
            },
            bytes,
        },
        state_owner: Some(id.into()),
        grows_memory: false,
    }
}

fn number_unit() -> RuntimeUnitOffer {
    psrs_linker::runtime::offer(&psrs_runtime::NUMBER_UNIT)
}

fn formatter() -> BindingRequirement {
    BindingRequirement {
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
        provider: Provider::RuntimeOperation {
            name: psrs_runtime::NUMBER_TO_STRING_OP.name.into(),
            version: psrs_runtime::NUMBER_TO_STRING_OP.version.into(),
        },
    }
}

fn require(unit: &mut RuntimeUnitOffer, operation: &str, expected: CoreSignature) {
    unit.required.push(RequiredOperation {
        name: operation.into(),
        version: "1".into(),
        signature: expected,
    });
}

#[test]
fn a_required_operation_selects_its_provider_before_the_root_unit() {
    let context = resolve_default_definitions().unwrap();
    let mut root = number_unit();
    require(&mut root, "psrs:dep/value", signature());
    let link = plan(
        &context,
        input(
            vec![formatter()],
            vec![root, dependency_unit("psrs:dep", "psrs:dep/value")],
        ),
    )
    .expect("the dependency should close");
    assert_eq!(
        link.units()
            .iter()
            .map(|unit| unit.id.as_str())
            .collect::<Vec<_>>(),
        ["psrs:dep", "psrs:runtime/number"]
    );
    assert!(link.edges().iter().any(|edge| matches!(
        edge,
        ProviderEdge::Dependency { unit, .. } if unit == "psrs:dep"
    )));
    assert_eq!(
        link.initialization(),
        &[
            InitializationStep::Instantiate {
                unit: "psrs:dep".into()
            },
            InitializationStep::Instantiate {
                unit: "psrs:runtime/number".into()
            },
            InitializationStep::ResolveShims,
        ]
    );
    let report = link.report();
    assert_eq!(report.memory_id, psrs_linker::APPLICATION_MEMORY);
    assert_eq!(report.growth_owner, psrs_linker::GENERATED_GROWTH_OWNER);
    assert_eq!(report.digests.len(), 2);
}

#[test]
fn an_unreached_unit_stays_out_of_the_plan() {
    let context = resolve_default_definitions().unwrap();
    let link = plan(
        &context,
        input(
            vec![formatter()],
            vec![number_unit(), dependency_unit("psrs:dep", "psrs:dep/value")],
        ),
    )
    .unwrap();
    assert_eq!(link.units().len(), 1);
    assert_eq!(link.artifacts().len(), 1);
}

#[test]
fn a_missing_provider_is_rejected() {
    let context = resolve_default_definitions().unwrap();
    let error = plan(&context, input(vec![formatter()], Vec::new()))
        .unwrap_err()
        .to_string();
    assert!(error.contains("no runtime unit provides"), "{error}");
    assert!(!error.contains("digest"), "{error}");
}

#[test]
fn a_missing_dependency_is_rejected() {
    let context = resolve_default_definitions().unwrap();
    let mut root = number_unit();
    require(&mut root, "psrs:missing/op", signature());
    let error = plan(&context, input(vec![formatter()], vec![root]))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("no runtime unit provides `psrs:missing/op@1`"),
        "{error}"
    );
}

#[test]
fn ambiguous_providers_are_not_resolved_by_trying_one() {
    let context = resolve_default_definitions().unwrap();
    let mut first = dependency_unit("psrs:left", "psrs:dep/value");
    let mut second = dependency_unit("psrs:right", "psrs:dep/value");
    first.artifact.contract.module_name = "psrs:dep".into();
    second.artifact.contract.module_name = "psrs:dep".into();
    let requirement = BindingRequirement {
        id: RequirementId(0),
        origin: "dep".into(),
        boundary: Boundary::RawCore {
            module: "psrs:dep".into(),
            field: "dep".into(),
        },
        expected: Some(signature()),
        provider: Provider::RuntimeOperation {
            name: "psrs:dep/value".into(),
            version: "1".into(),
        },
    };
    let error = plan(&context, input(vec![requirement], vec![first, second]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("ambiguous providers"), "{error}");
    assert!(!error.contains("digest"), "{error}");
}

#[test]
fn a_component_variant_is_not_substituted_for_a_raw_operation() {
    let context = resolve_default_definitions().unwrap();
    let mut unit = number_unit();
    unit.artifact.contract.kind = ArtifactKind::Component;
    let error = plan(&context, input(vec![formatter()], vec![unit]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("incompatible artifact kind"), "{error}");
    assert!(!error.contains("digest"), "{error}");
}

#[test]
fn the_core_provider_remains_selected_beside_a_component_advertisement() {
    let context = resolve_default_definitions().unwrap();
    let mut component = number_unit();
    component.id = "psrs:runtime/number-component".into();
    component.artifact.contract.id = "psrs:runtime-number-component".into();
    component.artifact.contract.kind = ArtifactKind::Component;
    component.state_owner = Some("psrs:runtime/number-component".into());
    let link = plan(
        &context,
        input(vec![formatter()], vec![number_unit(), component]),
    )
    .unwrap();
    assert_eq!(link.units().len(), 1);
    assert_eq!(link.units()[0].kind, ArtifactKind::CoreModule);
}

#[test]
fn a_provider_cycle_is_rejected() {
    let context = resolve_default_definitions().unwrap();
    let mut left = dependency_unit("psrs:left", "psrs:left/value");
    let mut right = dependency_unit("psrs:right", "psrs:right/value");
    require(&mut left, "psrs:right/value", signature());
    require(&mut right, "psrs:left/value", signature());
    let requirement = BindingRequirement {
        id: RequirementId(0),
        origin: "left".into(),
        boundary: Boundary::RawCore {
            module: "psrs:left".into(),
            field: "dep".into(),
        },
        expected: Some(signature()),
        provider: Provider::RuntimeOperation {
            name: "psrs:left/value".into(),
            version: "1".into(),
        },
    };
    let error = plan(&context, input(vec![requirement], vec![left, right]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("unsupported provider cycle"), "{error}");
}

#[test]
fn duplicate_state_owners_are_rejected() {
    let context = resolve_default_definitions().unwrap();
    let mut root = number_unit();
    require(&mut root, "psrs:dep/value", signature());
    let mut dependency = dependency_unit("psrs:dep", "psrs:dep/value");
    dependency.state_owner = root.state_owner.clone();
    let error = plan(&context, input(vec![formatter()], vec![root, dependency]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("duplicate state owner"), "{error}");
}

#[test]
fn the_demand_may_name_a_runtime_unit_as_the_growth_owner() {
    let context = resolve_default_definitions().unwrap();
    let allocator = psrs_linker::runtime::offer(&psrs_runtime::ALLOCATOR_UNIT);
    let requirement = BindingRequirement {
        id: RequirementId(0),
        origin: "allocator".into(),
        boundary: Boundary::RawCore {
            module: psrs_runtime::ALLOCATOR_MODULE.into(),
            field: psrs_runtime::REALLOC_EXPORT.into(),
        },
        expected: Some(CoreSignature {
            parameters: vec![CoreType::I32; 4],
            result: Some(CoreType::I32),
        }),
        provider: Provider::RuntimeOperation {
            name: psrs_runtime::ALLOCATOR_REALLOC_OP.name.into(),
            version: psrs_runtime::ALLOCATOR_REALLOC_OP.version.into(),
        },
    };
    let mut input = input(vec![requirement], vec![allocator]);
    input.memory.growth_owner = psrs_runtime::ALLOCATOR_UNIT.id.into();
    let link = plan(&context, input).expect("the named unit should own growth");
    assert_eq!(link.growth_owner(), psrs_runtime::ALLOCATOR_UNIT.id);
    assert!(link.memory().heap_getter.is_some());
}

#[test]
fn a_second_growth_owner_is_rejected() {
    let context = resolve_default_definitions().unwrap();
    let mut root = number_unit();
    require(&mut root, "psrs:dep/value", signature());
    let mut dependency = dependency_unit("psrs:dep", "psrs:dep/value");
    dependency.grows_memory = true;
    let error = plan(&context, input(vec![formatter()], vec![root, dependency]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("conflicting growth owners"), "{error}");
}

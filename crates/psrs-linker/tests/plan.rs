//! Target-only linker tests: no compiler IR is constructed.

use psrs_linker::{
    BindingRequirement, Boundary, CoreSignature, CoreType, MemoryDemand, Provider, RequirementId,
    RuntimeUnitOffer, TargetLinkInput, TargetPolicy, plan, resolve_default_definitions,
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

fn number_unit() -> RuntimeUnitOffer {
    psrs_linker::runtime::offer(&psrs_runtime::NUMBER_UNIT)
}

fn formatter_requirement() -> BindingRequirement {
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

fn stdout_requirement(id: u32) -> BindingRequirement {
    BindingRequirement {
        id: RequirementId(id),
        origin: "wasi:cli/stdout.get-stdout".into(),
        boundary: Boundary::ResolvedWit {
            interface: "wasi:cli/stdout@0.2.12".into(),
            function: "get-stdout".into(),
        },
        expected: Some(CoreSignature {
            parameters: Vec::new(),
            result: Some(CoreType::I32),
        }),
        provider: Provider::HostInterface {
            interface: "wasi:cli/stdout@0.2.12".into(),
        },
    }
}

#[test]
fn a_live_formatter_requirement_reserves_storage_and_closes_the_private_import() {
    let context = resolve_default_definitions().unwrap();
    let link = plan(
        &context,
        input(
            vec![formatter_requirement(), stdout_requirement(1)],
            vec![number_unit()],
        ),
    )
    .expect("the formatter plan should be valid");

    assert_eq!(link.artifacts().len(), 1);
    assert_eq!(link.memory().heap_start, psrs_runtime::HEAP_START);
    assert!(link.memory().minimum_pages >= 4);
    assert_eq!(
        link.import(RequirementId(0)).unwrap().module,
        psrs_runtime::MODULE_NAME
    );
    // The encoder retains only the live resource owner, not unused methods'
    // dependencies (`error` and `poll` belong to other streams operations).
    assert_eq!(
        link.external_world(),
        &["wasi:cli/stdout@0.2.12", "wasi:io/streams@0.2.12"]
    );
    assert_eq!(link.digests().len(), 1);
    assert_eq!(
        link.digests()[0],
        psrs_runtime::NUMBER_RUNTIME.provenance.sha256
    );
}

#[test]
fn an_unused_implementation_contributes_neither_bytes_nor_storage() {
    let context = resolve_default_definitions().unwrap();
    let link = plan(&context, input(Vec::new(), Vec::new())).expect("an empty plan is valid");
    assert!(link.artifacts().is_empty());
    assert!(link.memory().heap_start < psrs_runtime::HEAP_START);
}

#[test]
fn a_missing_artifact_provider_is_rejected() {
    let context = resolve_default_definitions().unwrap();
    let result = plan(&context, input(vec![formatter_requirement()], Vec::new()));
    assert!(result.is_err(), "an absent artifact must not resolve");
}

#[test]
fn a_host_interface_outside_the_world_is_rejected() {
    let context = resolve_default_definitions().unwrap();
    let requirement = BindingRequirement {
        id: RequirementId(0),
        origin: "wasi:http/outgoing-handler.handle".into(),
        boundary: Boundary::ResolvedWit {
            interface: "wasi:http/outgoing-handler@0.2.12".into(),
            function: "handle".into(),
        },
        expected: Some(CoreSignature {
            parameters: Vec::new(),
            result: Some(CoreType::I32),
        }),
        provider: Provider::HostInterface {
            interface: "wasi:http/outgoing-handler@0.2.12".into(),
        },
    };
    assert!(
        plan(&context, input(vec![requirement], Vec::new())).is_err(),
        "a definition outside the world must not satisfy a live import"
    );
}

#[test]
fn a_world_interface_disabled_by_the_target_profile_is_rejected() {
    let context = resolve_default_definitions().unwrap();
    // `wasi:cli/stdout` is in the world but not in this restricted policy.
    let result = plan(
        &context,
        TargetLinkInput {
            requirements: vec![stdout_requirement(0)],
            units: Vec::new(),
            policy: TargetPolicy {
                permitted_host_interfaces: vec!["wasi:io/streams@0.2.12".into()],
            },
            memory: memory(),
        },
    );
    assert!(
        result.is_err(),
        "the target capability profile must gate a world interface"
    );
}

#[test]
fn a_different_world_version_does_not_satisfy_a_pinned_import() {
    let context = resolve_default_definitions().unwrap();
    let requirement = BindingRequirement {
        id: RequirementId(0),
        origin: "wasi:cli/stdout@0.2.11.get-stdout".into(),
        boundary: Boundary::ResolvedWit {
            interface: "wasi:cli/stdout@0.2.11".into(),
            function: "get-stdout".into(),
        },
        expected: Some(CoreSignature {
            parameters: Vec::new(),
            result: Some(CoreType::I32),
        }),
        provider: Provider::HostInterface {
            interface: "wasi:cli/stdout@0.2.11".into(),
        },
    };
    assert!(
        plan(&context, input(vec![requirement], Vec::new())).is_err(),
        "a pinned import version must match the resolved world exactly"
    );
}

#[test]
fn a_reservation_overlapping_canonical_state_is_rejected() {
    let context = resolve_default_definitions().unwrap();
    let mut unit = number_unit();
    let storage = unit.artifact.contract.storage.as_mut().unwrap();
    storage.static_data.start = 16;
    let result = plan(&context, input(vec![formatter_requirement()], vec![unit]));
    assert!(result.is_err(), "overlapping reservations must be rejected");
}

#[test]
fn conflicting_providers_for_one_import_identity_are_rejected() {
    let context = resolve_default_definitions().unwrap();
    let mut other = formatter_requirement();
    other.id = RequirementId(1);
    other.origin = "NumberToStringAgain".into();
    other.expected = Some(CoreSignature {
        // A deliberately different signature for the same import identity.
        parameters: vec![CoreType::F64, CoreType::I32],
        result: Some(CoreType::I32),
    });
    assert!(
        plan(
            &context,
            input(vec![formatter_requirement(), other], vec![number_unit()])
        )
        .is_err(),
        "two signatures for one import identity must be rejected"
    );
}

/// A definition-only contract is not an executable provider; verification of a
/// mismatched contract must fail before any plan is published.
#[test]
fn a_definition_contract_does_not_satisfy_execution() {
    let mut unit = number_unit();
    unit.artifact.contract.exports.clear();
    let artifact = unit;
    let context = resolve_default_definitions().unwrap();
    assert!(
        plan(
            &context,
            input(vec![formatter_requirement()], vec![artifact])
        )
        .is_err()
    );
}

#[test]
fn the_policy_cannot_introduce_an_interface_absent_from_the_world() {
    let context = resolve_default_definitions().unwrap();
    let mut requirement = stdout_requirement(0);
    let interface = "wasi:cli/stdout@99.0.0";
    requirement.boundary = Boundary::ResolvedWit {
        interface: interface.into(),
        function: "get-stdout".into(),
    };
    requirement.provider = Provider::HostInterface {
        interface: interface.into(),
    };
    let mut input = input(vec![requirement], Vec::new());
    input
        .policy
        .permitted_host_interfaces
        .push(interface.into());
    assert!(
        plan(&context, input)
            .unwrap_err()
            .to_string()
            .contains("outside the resolved world")
    );
}

#[test]
fn impossible_memory_demands_are_rejected_before_arithmetic() {
    let context = resolve_default_definitions().unwrap();
    for (range, alignment) in [((16, 0), 8), ((0, 16), 3), ((0, 16), 0)] {
        let mut input = input(Vec::new(), Vec::new());
        input.memory.canonical_scratch = range;
        input.memory.heap_alignment = alignment;
        assert!(plan(&context, input).is_err());
    }
    let mut unit = number_unit();
    unit.artifact
        .contract
        .storage
        .as_mut()
        .unwrap()
        .minimum_pages = u64::MAX;
    assert!(plan(&context, input(vec![formatter_requirement()], vec![unit])).is_err());
}

#[test]
fn a_resource_owner_disabled_by_policy_cannot_hide_behind_a_live_method() {
    let context = resolve_default_definitions().unwrap();
    let mut input = input(vec![stdout_requirement(0)], Vec::new());
    input.policy.permitted_host_interfaces = vec!["wasi:cli/stdout@0.2.12".into()];
    assert!(
        plan(&context, input)
            .unwrap_err()
            .to_string()
            .contains("resource dependency")
    );
}

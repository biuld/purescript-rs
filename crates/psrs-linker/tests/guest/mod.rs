mod fixtures;
use psrs_linker::guest::{
    ComponentLinkInput, ComponentReference, GuestBinding, compose_component, plan_components,
};
use psrs_linker::{TargetPolicy, sha256_hex};

fn reference(id: &str, bytes: Vec<u8>) -> ComponentReference {
    ComponentReference {
        id: id.into(),
        sha256: sha256_hex(&bytes),
        bytes,
    }
}

fn fixture_input() -> ComponentLinkInput {
    let (application, provider) = fixtures::components();
    ComponentLinkInput {
        application: reference("application", application),
        guests: vec![reference("provider", provider)],
        bindings: vec![GuestBinding {
            interface: fixtures::INTERFACE.into(),
            artifact: "provider".into(),
            export: fixtures::INTERFACE.into(),
        }],
        policy: TargetPolicy::default(),
        features: wasmparser::WasmFeatures::default(),
    }
}

#[test]
fn strings_lists_results_post_return_and_resource_lifetimes_execute() {
    let input = fixture_input();
    let application = input.application.bytes.clone();
    let plan = plan_components(input).expect("typed guest interfaces should connect");
    assert!(plan.external_world().is_empty());
    let linked = compose_component(&plan, &application).unwrap();
    let path = std::env::temp_dir().join(format!("psrs-guest-{}.wasm", std::process::id()));
    std::fs::write(&path, &linked.bytes).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("Wasmtime is required for guest composition acceptance");
    std::fs::remove_file(&path).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn stale_guest_and_changed_application_are_rejected() {
    let mut input = fixture_input();
    input.guests[0].sha256 = "0".repeat(64);
    assert!(
        plan_components(input)
            .unwrap_err()
            .to_string()
            .contains("digest")
    );
    let input = fixture_input();
    let plan = plan_components(input).unwrap();
    assert!(compose_component(&plan, b"other application").is_err());
}

#[test]
fn missing_guest_and_version_drift_never_fall_back_to_the_host() {
    let mut input = fixture_input();
    input.guests.clear();
    input
        .policy
        .permitted_host_interfaces
        .push(fixtures::INTERFACE.into());
    assert!(plan_components(input).is_err());
    let mut input = fixture_input();
    input.bindings[0].export = "test:lib/api@2.0.0".into();
    assert!(plan_components(input).is_err());
}

#[test]
fn an_incompatible_guest_interface_is_rejected() {
    let mut input = fixture_input();
    let bytes = fixtures::incompatible_provider();
    input.guests[0] = reference("provider", bytes);
    assert!(plan_components(input).is_err());
}

const API: &str = "test:graph/api@1.0.0";
const DEP: &str = "test:graph/dependency@1.0.0";
const HOST: &str = "test:graph/host@1.0.0";

fn graph_input() -> ComponentLinkInput {
    ComponentLinkInput {
        application: reference("application", fixtures::forwarding_component(API, API)),
        guests: vec![reference(
            "provider",
            fixtures::forwarding_component(HOST, API),
        )],
        bindings: vec![GuestBinding {
            interface: API.into(),
            artifact: "provider".into(),
            export: API.into(),
        }],
        policy: TargetPolicy {
            permitted_host_interfaces: vec![HOST.into()],
        },
        features: wasmparser::WasmFeatures::default(),
    }
}

#[test]
fn only_used_guest_dependencies_contribute_exact_residual_host_imports() {
    let mut input = graph_input();
    // Unreachable artifacts are neither validated nor included in the closure.
    input
        .guests
        .push(reference("unused", b"not an executable component".to_vec()));
    let plan = plan_components(input).unwrap();
    assert_eq!(plan.external_world(), &[HOST]);
    assert_eq!(plan.component_artifacts().len(), 2);
    let mut input = graph_input();
    input.policy.permitted_host_interfaces.clear();
    assert!(
        plan_components(input)
            .unwrap_err()
            .to_string()
            .contains(HOST)
    );
}

#[test]
fn transitive_guest_dependencies_close_without_host_fallback() {
    let mut input = graph_input();
    input.guests[0] = reference("provider", fixtures::forwarding_component(DEP, API));
    input
        .guests
        .push(reference("dependency", fixtures::constant_component(DEP)));
    input.bindings.push(GuestBinding {
        interface: DEP.into(),
        artifact: "dependency".into(),
        export: DEP.into(),
    });
    let plan = plan_components(input).unwrap();
    assert!(plan.external_world().is_empty());
    assert_eq!(plan.component_artifacts().len(), 3);
}

#[test]
fn provider_conflicts_missing_exports_and_dependency_cycles_fail_closed() {
    let mut input = graph_input();
    input.bindings.push(input.bindings[0].clone());
    assert!(
        plan_components(input)
            .unwrap_err()
            .to_string()
            .contains("conflicting")
    );
    let mut input = graph_input();
    input.guests[0] = reference("provider", fixtures::constant_component(DEP));
    assert!(
        plan_components(input)
            .unwrap_err()
            .to_string()
            .contains("omits selected export")
    );
    let mut input = graph_input();
    input.guests[0] = reference("provider", fixtures::forwarding_component(DEP, API));
    input.guests.push(reference(
        "dependency",
        fixtures::forwarding_component(API, DEP),
    ));
    input.bindings.push(GuestBinding {
        interface: DEP.into(),
        artifact: "dependency".into(),
        export: DEP.into(),
    });
    assert!(plan_components(input).is_err());
}

#[test]
fn component_features_are_validated_against_the_selected_profile() {
    let mut input = graph_input();
    input
        .features
        .remove(wasmparser::WasmFeatures::COMPONENT_MODEL);
    assert!(plan_components(input).is_err());
}

#[test]
fn definition_bytes_cannot_supply_an_executable_interface() {
    let mut input = fixture_input();
    input.guests[0] = reference("provider", fixtures::definition_only_package());
    assert!(plan_components(input).is_err());
}

#[test]
fn guest_executable_initialization_requires_an_explicit_contract() {
    let mut input = fixture_input();
    let provider = wat::parse_str(format!(
        r#"(component
        (core module $m (func $start) (start $start))
        (core instance $m (instantiate $m))
        (instance $api)
        (export "{}" (instance $api)))"#,
        fixtures::INTERFACE
    ))
    .unwrap();
    input.guests = vec![reference("provider", provider)];
    let error = plan_components(input).unwrap_err().to_string();
    assert!(error.contains("initialization"), "{error}");
}

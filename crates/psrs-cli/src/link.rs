//! Explicit pinned guest-provider composition for an application component.
use psrs_linker::guest::{
    ComponentLinkInput, ComponentReference, GuestBinding, compose_component, plan_components,
};
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    application_sha256: Option<String>,
    providers: Vec<Artifact>,
    bindings: Vec<Binding>,
    permitted_host_interfaces: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    id: String,
    path: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    interface: String,
    provider: String,
    export: Option<String>,
}

pub fn run(args: Vec<String>) -> Result<(), String> {
    let mut application = None;
    let mut manifest = None;
    let mut output = None;
    let mut report = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let slot = match arg.as_str() {
            "--manifest" => &mut manifest,
            "-o" => &mut output,
            "--report" => &mut report,
            value if !value.starts_with('-') && application.is_none() => {
                application = Some(arg);
                continue;
            }
            _ => return Err(usage()),
        };
        if slot.is_some() {
            return Err(usage());
        }
        *slot = Some(args.next().ok_or_else(usage)?);
    }
    let application = application.ok_or_else(usage)?;
    let manifest_path = manifest.ok_or_else(usage)?;
    let output = output.ok_or_else(usage)?;
    let application_bytes = read(&application)?;
    let (bytes, evidence) = compose(&application_bytes, &manifest_path, true)?;
    if let Some(report) = report {
        write_report(&report, &evidence)?;
    }
    fs::write(&output, bytes).map_err(|error| format!("{output}: {error}"))
}

/// Compiler-produced roots are pinned by the bytes emitted in this invocation.
/// Standalone inputs require an explicit manifest root pin.
pub(super) fn compose(
    application_bytes: &[u8],
    manifest_path: &str,
    require_root_pin: bool,
) -> Result<(Vec<u8>, serde_json::Value), String> {
    let manifest_bytes = read(manifest_path)?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("{manifest_path}: {error}"))?;
    if manifest.schema_version != 1 {
        return Err("unsupported link manifest schema".into());
    }
    let target = psrs_driver::TargetCapabilities::default();
    let world = psrs_linker::resolve_default_definitions().map_err(|error| error.to_string())?;
    for interface in &manifest.permitted_host_interfaces {
        if interface.starts_with("wasi:")
            && (!world.imports_interface(interface) || !target.wasi_interface_enabled(interface))
        {
            return Err(format!(
                "host interface `{interface}` is outside the selected target profile"
            ));
        }
    }
    let base = Path::new(manifest_path)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let guests = manifest
        .providers
        .into_iter()
        .map(|artifact| {
            let path = base.join(&artifact.path);
            Ok(ComponentReference {
                id: artifact.id,
                sha256: artifact.sha256,
                bytes: fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bindings = manifest
        .bindings
        .into_iter()
        .map(|binding| GuestBinding {
            export: binding.export.unwrap_or_else(|| binding.interface.clone()),
            interface: binding.interface,
            artifact: binding.provider,
        })
        .collect();
    let plan = plan_components(ComponentLinkInput {
        application: ComponentReference {
            id: "application".into(),
            sha256: match manifest.application_sha256 {
                Some(pin) => pin,
                None if !require_root_pin => psrs_linker::sha256_hex(application_bytes),
                None => return Err("link requires application_sha256".into()),
            },
            bytes: application_bytes.to_vec(),
        },
        guests,
        bindings,
        policy: psrs_linker::TargetPolicy {
            permitted_host_interfaces: manifest.permitted_host_interfaces,
        },
        features: target.wasm_features(),
    })
    .map_err(|error| error.to_string())?;
    let linked = compose_component(&plan, application_bytes).map_err(|error| error.to_string())?;
    let evidence = serde_json::json!({
        "schema_version":1,"manifest_sha256":psrs_linker::sha256_hex(&manifest_bytes),"component_artifacts":plan.component_artifacts(),
        "bindings":plan.bindings().iter().map(|binding| serde_json::json!({
            "consumer":binding.consumer,"interface":binding.interface,
            "provider":binding.provider,"export":binding.export,
        })).collect::<Vec<_>>(),
        "target_features":plan.features().bits(),
        "external_world":linked.external_world,"output_sha256":psrs_linker::sha256_hex(&linked.bytes),
    });
    Ok((linked.bytes, evidence))
}

pub(super) fn write_report(path: &str, evidence: &serde_json::Value) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(evidence).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("{path}: {error}"))
}

fn read(path: &str) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("{path}: {error}"))
}

fn usage() -> String {
    "usage: psrs link application.wasm --manifest providers.json -o linked.wasm [--report report.json]".into()
}

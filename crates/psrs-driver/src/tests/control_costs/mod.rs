//! Cost evidence combines actual ancestry guards with observed GC collections.

mod probe;
mod sources;

use std::process::Command;

fn run(name: &str, template: &str, steps: i32, depth: i32, expected: i32) -> Option<Vec<u64>> {
    super::wasmtime_available()?;
    let source = template.replace("STEPS", &steps.to_string());
    let mut stages =
        crate::compile_main_stages(&source).expect("cost source compiles and verifies");
    assert!(
        probe::instrument(&mut stages, depth) > 0,
        "no extent allocations inspected"
    );
    let core =
        psrs_backend::wasm::encode_module(&stages.wasm).expect("instrumented target encodes");
    let bytes = probe::component(&stages, &core);
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .expect("instrumented Wasm validates");
    let artifact = psrs_backend::Artifact {
        wasm: bytes,
        wat: String::new(),
        warnings: vec![],
    };
    let prefix = std::env::temp_dir().join(format!("psrs-control-cost-{name}-{steps}-{depth}"));
    let wasm = prefix.with_extension("wasm");
    std::fs::write(prefix.with_extension("purs"), source).unwrap();
    let identity = artifact
        .wasm
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    std::fs::write(&wasm, artifact.wasm).unwrap();
    let output = Command::new("wasmtime")
        .args(["run", "-O", "gc-heap-initial-size=65536"])
        .env("WASMTIME_LOG", "wasmtime::runtime::vm::gc=trace")
        .arg(&wasm)
        .output()
        .unwrap();
    std::fs::write(prefix.with_extension("gc.log"), &output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(expected), "{name}: {output:?}");
    let log = String::from_utf8_lossy(&output.stderr);
    if expected == 134 {
        assert!(
            log.contains("wasm trap:") && !log.contains("cast failure"),
            "{log}"
        );
    }
    let bytes = log
        .lines()
        .filter_map(|line| {
            let (_, after) = line.split_once("After collection, GC heap's allocated bytes = 0x")?;
            let hex = after.split_whitespace().next().unwrap();
            Some(u64::from_str_radix(hex, 16).expect("observed GC live bytes"))
        })
        .collect::<Vec<_>>();
    let report = serde_json::json!({
        "workload": name,
        "steps": steps,
        "ancestry_bound": depth,
        "observed_exit": output.status.code(),
        "artifact": wasm,
        "artifact_identity": format!("fnv1a64-v1:{identity:016x}"),
        "gc_initial_heap_bytes": 65536,
        "collections": bytes.len(),
        "maximum_post_gc_live_bytes": bytes.iter().max(),
        "minimum_post_gc_live_bytes": bytes.iter().min(),
    });
    std::fs::write(
        prefix.with_extension("json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    eprintln!(
        "{name}: steps={steps}, ancestry_bound={depth}, collections={}, maximum_post_gc_live_bytes={}",
        bytes.len(),
        bytes.iter().max().unwrap_or(&0)
    );
    Some(bytes)
}

#[test]
fn control_tail_handlers_bound_extent_ancestry_and_post_gc_live_heap() {
    for (name, source) in [("deep", sources::DEEP), ("forwarded", sources::FORWARDED)] {
        let mut previous = 0;
        for steps in [1000, 10000, 100000] {
            let Some(bytes) = run(name, source, steps, 2, 42) else {
                return;
            };
            assert!(
                !bytes.is_empty(),
                "no actual collection at {steps} requests"
            );
            assert!(
                bytes.len() > previous,
                "increased work must exercise collection churn"
            );
            previous = bytes.len();
            assert!(
                bytes.iter().all(|n| *n < 16384),
                "live heap grows despite tail extent coalescing: {bytes:?}"
            );
        }
    }
}

#[test]
fn control_ancestry_probe_observes_independent_non_tail_extents() {
    // This distinguishes an actual ancestry walk from a dead or ineffective probe.
    run("nested", sources::NESTED, 8, 8, 42);
    run("nested", sources::NESTED, 8, 7, 134);
}

#[test]
fn control_cost_pipeline_rejects_a_malformed_compiler_input() {
    let mut input = crate::lower_main_to_cc(&sources::DEEP.replace("STEPS", "1")).unwrap();
    let symbol = input
        .cc
        .representations
        .control_helpers
        .iter()
        .find(|h| h.role() == psrs_backend::cc::ControlHelperRole::Executor)
        .unwrap()
        .function();
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    let parameter = function.parameters[2];
    function
        .values
        .iter_mut()
        .find(|v| v.id == parameter)
        .unwrap()
        .ty = psrs_backend::cc::ValueShape::Integer;
    let errors = psrs_backend::compile_cc(input).expect_err("CC must be rechecked before emission");
    assert!(
        errors.iter().any(|error| error.pass == "P8 CC verification"
            && error.kind == psrs_backend::BackendErrorKind::InvalidCompilerIr),
        "{errors:?}"
    );
}

//! Runtime observations of optimized Wasm. Compile each source once, retain
//! identified artifacts, and require the exact success/trapping budget boundary.
use super::{probe, sources};
use psrs_backend::Stages;
use std::{path::PathBuf, process::Command};

#[derive(Clone, Copy, Debug)]
enum Counter {
    Entries,
    Steps(Option<i32>),
}

impl Counter {
    fn label(self) -> String {
        match self {
            Self::Entries => "entries".into(),
            Self::Steps(None) => "steps-all".into(),
            Self::Steps(Some(n)) => format!("steps-replay-{n}"),
        }
    }
}

struct Program {
    stages: Stages,
    directory: PathBuf,
    ancestry: i32,
}

fn identity(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("fnv1a64-v1:{hash:016x}")
}

impl Program {
    fn compile(name: &str, source: &str, ancestry: i32) -> Option<Self> {
        super::super::wasmtime_available()?;
        let mut stages = crate::compile_main_stages(source)
            .expect("cost source compiles through the verified pipeline");
        let directory = std::env::temp_dir()
            .join(format!("psrs-control-replay-costs-{}", std::process::id()))
            .join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("source.purs"), source).unwrap();
        std::fs::write(directory.join("base.wasm"), &stages.artifact.wasm).unwrap();
        let base_identity = identity(&stages.artifact.wasm);
        assert!(
            probe::instrument(&mut stages, ancestry) > 0,
            "ancestry observation must inspect actual allocations"
        );
        let program = Self {
            stages,
            directory,
            ancestry,
        };
        assert!(
            program.run(&program.stages, "ancestry").0,
            "unlimited ancestry-instrumented program must return 42"
        );
        let version = Command::new("wasmtime").arg("--version").output().unwrap();
        let report = serde_json::json!({"source":program.directory.join("source.purs"),"source_identity":identity(source.as_bytes()),"base_artifact_identity":base_identity,"ancestry_bound":ancestry,"wasmtime":String::from_utf8_lossy(&version.stdout).trim()});
        std::fs::write(
            program.directory.join("input.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        Some(program)
    }

    /// Return success only for the checked value. A non-budget failure must not
    /// become a measured count or look like an unavailable runtime skip.
    fn run(&self, stages: &Stages, label: &str) -> (bool, String) {
        let core =
            psrs_backend::wasm::encode_module(&stages.wasm).expect("observed target encodes");
        let bytes = probe::component(stages, &core);
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
            .validate_all(&bytes)
            .expect("observed component validates");
        let hash = identity(&bytes);
        let path = self.directory.join(format!("{label}.wasm"));
        std::fs::write(&path, bytes).unwrap();
        let output = Command::new("wasmtime")
            .arg("run")
            .arg(&path)
            .output()
            .expect("run observed component");
        std::fs::write(
            self.directory.join(format!("{label}.stderr")),
            &output.stderr,
        )
        .unwrap();
        match output.status.code() {
            Some(42) => (true, hash),
            Some(134) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(
                    stderr.contains("wasm trap:")
                        && stderr.contains("unreachable")
                        && !stderr.contains("cast failure"),
                    "unexpected failure while measuring {path:?}: {output:?}"
                );
                (false, hash)
            }
            _ => panic!(
                "value/order/request-count assertion failed while measuring {path:?}: {output:?}"
            ),
        }
    }

    fn budget(&self, counter: Counter, budget: i32) -> (bool, String) {
        let mut stages = self.stages.clone();
        let inspected = match counter {
            Counter::Entries => probe::instrument_replay(&mut stages, budget),
            Counter::Steps(ordinal) => probe::instrument_replay_steps(&mut stages, budget, ordinal),
        };
        assert_eq!(inspected, 2, "both replay helpers must be observed");
        self.run(&stages, &format!("{}-budget-{budget}", counter.label()))
    }

    fn measure(&self, counter: Counter) -> i32 {
        let mut high = 1;
        while !self.budget(counter, high).0 {
            high *= 2;
            assert!(
                high <= 1 << 20,
                "replay did not complete within the observation budget"
            );
        }
        let mut low = 0;
        while low < high {
            let middle = low + (high - low) / 2;
            if self.budget(counter, middle).0 {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        assert!(low > 0, "a replay observation must execute");
        let (success, successful_identity) = self.budget(counter, low);
        let (under_budget, trapping_identity) = self.budget(counter, low - 1);
        assert!(
            success && !under_budget,
            "counter must distinguish count from count-1"
        );
        let report = serde_json::json!({"counter":counter.label(),"count":low,"ancestry_bound":self.ancestry,"successful_budget":low,"successful_exit":42,"successful_artifact_identity":successful_identity,"trapping_budget":low-1,"trapping_exit":134,"trapping_artifact_identity":trapping_identity});
        std::fs::write(
            self.directory.join(format!("{}.json", counter.label())),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        eprintln!(
            "{}: {} = {low}, ancestry <= {}",
            self.directory.display(),
            counter.label(),
            self.ancestry
        );
        low
    }
}

pub(super) fn entry_invocations(name: &str, template: &str, steps: i32) -> Option<i32> {
    let source = template.replace("STEPS", &steps.to_string());
    let program = Program::compile(&format!("{name}-requests-{steps}"), &source, 2)?;
    Some(program.measure(Counter::Entries))
}

#[test]
fn saved_callback_length_changes_actual_loop_work_at_fixed_requests_and_ancestry() {
    for length in [0, 1, 4, 16] {
        let source = sources::SAVED_LENGTH.replace("LENGTH", &length.to_string());
        let Some(program) = Program::compile(&format!("linear-length-{length}"), &source, 0) else {
            return;
        };
        assert_eq!(
            program.measure(Counter::Entries),
            2,
            "one replay has two helper entries regardless of length"
        );
        assert_eq!(
            program.measure(Counter::Steps(None)),
            2 * length + 2,
            "both loops must visit every saved callback and their terminal Empty"
        );
    }
}

#[test]
fn queue_shape_changes_traversal_with_equal_callbacks_requests_and_ancestry() {
    for length in [0, 4, 16] {
        for (name, source, extra) in [
            ("linear", sources::LINEAR_SECOND, 0),
            ("append", sources::APPEND_SECOND, 2),
        ] {
            let source = source.replace("LENGTH", &length.to_string());
            let Some(program) =
                Program::compile(&format!("second-{name}-length-{length}"), &source, 0)
            else {
                return;
            };
            assert_eq!(
                program.measure(Counter::Entries),
                4,
                "both programs have exactly two replays"
            );
            assert_eq!(
                program.measure(Counter::Steps(Some(2))),
                2 * length + 2 + extra,
                "the second captured sequence has {length} callbacks; Append adds a stack push and empty right traversal"
            );
        }
    }
}

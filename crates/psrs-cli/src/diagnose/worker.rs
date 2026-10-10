use super::{
    CaseTrace, DiagnosticRecord, SourceInput, atomic_write, empty_case_trace, fingerprint_sources,
    first_line, from_report,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{env, fs, thread};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct WorkerRequest {
    pub(super) path: String,
    pub(super) category_dir: Option<String>,
    #[serde(default)]
    pub(super) explicit_inputs: Vec<String>,
    #[serde(default)]
    pub(super) capture_dumps: bool,
    #[serde(default)]
    pub(super) trusted_stdlib_fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct WorkerResponse {
    pub(super) passed: bool,
    pub(super) input_fingerprint: String,
    pub(super) sources: Vec<SourceInput>,
    pub(super) input_set_complete: bool,
    pub(super) elapsed_ms: u64,
    pub(super) diagnostics: Vec<DiagnosticRecord>,
    pub(super) core_stage: Option<String>,
    pub(super) core: Option<String>,
    pub(super) cc_stage: Option<String>,
    pub(super) cc: Option<String>,
    pub(super) mir_stage: Option<String>,
    pub(super) mir: Option<String>,
    pub(super) trace: Option<CaseTrace>,
}

#[derive(Debug)]
pub(super) enum WorkerOutcome {
    Completed(Box<WorkerResponse>),
    TimedOut {
        elapsed_ms: u64,
    },
    Crashed {
        message: String,
        stderr: String,
        elapsed_ms: u64,
    },
}

pub fn worker(request_path: &str, response_path: &str) -> Result<(), String> {
    let request_bytes =
        fs::read(request_path).map_err(|error| format!("{request_path}: {error}"))?;
    let request: WorkerRequest = serde_json::from_slice(&request_bytes)
        .map_err(|error| format!("{request_path}: invalid worker request: {error}"))?;
    let started = Instant::now();
    let entry_path = PathBuf::from(&request.path);
    let entry_text = match fs::read_to_string(&entry_path) {
        Ok(text) => text,
        Err(error) => {
            let response = WorkerResponse {
                passed: false,
                input_fingerprint: fingerprint_sources(&[]),
                sources: Vec::new(),
                input_set_complete: false,
                elapsed_ms: started.elapsed().as_millis() as u64,
                diagnostics: vec![DiagnosticRecord {
                    origin: "program".into(),
                    source: Some(request.path),
                    stage: "input loading".into(),
                    start: 0,
                    end: 0,
                    code: None,
                    kind: None,
                    message: format!("{}: {error}", entry_path.display()),
                }],
                core_stage: None,
                core: None,
                cc_stage: None,
                cc: None,
                mir_stage: None,
                mir: None,
                trace: None,
            };
            return atomic_write(
                Path::new(response_path),
                &serde_json::to_vec(&response).map_err(|error| error.to_string())?,
            );
        }
    };
    let loaded = if let Some(category) = request.category_dir.as_deref() {
        psrs_driver::load_program_case_sources(&entry_path, Path::new(category), &entry_text)
            .map(|case| case.own.into_iter().chain(case.loaded).collect::<Vec<_>>())
    } else if request.explicit_inputs.is_empty() {
        psrs_driver::load_program_files(std::slice::from_ref(&request.path))
    } else {
        request
            .explicit_inputs
            .iter()
            .map(|path| {
                fs::read_to_string(path)
                    .map(|text| (path.clone(), text))
                    .map_err(|error| format!("{path}: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()
    };
    let (sources, input_set_complete, load_error) = match loaded {
        Ok(sources) => (sources, true, None),
        Err(error) => (vec![(request.path.clone(), entry_text)], false, Some(error)),
    };
    let inputs = sources
        .iter()
        .map(|(name, text)| SourceInput {
            name: name.clone(),
            text: text.clone(),
        })
        .collect::<Vec<_>>();
    if let Some(error) = load_error {
        let input_fingerprint = fingerprint_sources(&inputs);
        let trace = empty_case_trace(
            &inputs,
            input_set_complete,
            &input_fingerprint,
            request.capture_dumps,
        );
        let response = WorkerResponse {
            passed: false,
            input_fingerprint,
            sources: inputs,
            input_set_complete,
            elapsed_ms: started.elapsed().as_millis() as u64,
            diagnostics: vec![DiagnosticRecord {
                origin: "program".into(),
                source: Some(request.path),
                stage: "input loading".into(),
                start: 0,
                end: 0,
                code: None,
                kind: None,
                message: error,
            }],
            core_stage: None,
            core: None,
            cc_stage: None,
            cc: None,
            mir_stage: None,
            mir: None,
            trace: Some(trace),
        };
        return atomic_write(
            Path::new(response_path),
            &serde_json::to_vec(&response).map_err(|error| error.to_string())?,
        );
    }
    let sources = inputs;
    let source_refs = sources
        .iter()
        .map(|source| (source.name.as_str(), source.text.as_str()))
        .collect::<Vec<_>>();
    let report = psrs_driver::compile_program_sources_with_prelude_diagnosis(
        &source_refs,
        request.capture_dumps,
    );
    let input_fingerprint = fingerprint_sources(&sources);
    let trace = from_report(
        &sources,
        input_set_complete,
        &input_fingerprint,
        &request.trusted_stdlib_fingerprint,
        request.capture_dumps,
        &report,
    )?;
    let diagnostics = report
        .diagnostics
        .iter()
        .map(|item| {
            let (origin, source) = match item.source {
                psrs_driver::DiagnosticOrigin::Source(index) => (
                    "source".to_owned(),
                    sources.get(index).map(|source| source.name.clone()),
                ),
                psrs_driver::DiagnosticOrigin::Library => ("library".to_owned(), None),
                psrs_driver::DiagnosticOrigin::Program => ("program".to_owned(), None),
            };
            DiagnosticRecord {
                origin,
                source,
                stage: item.diagnostic.stage.to_owned(),
                start: item.diagnostic.span.start,
                end: item.diagnostic.span.end,
                code: item.diagnostic.code.map(str::to_owned),
                kind: item.diagnostic.kind.map(|kind| format!("{kind:?}")),
                message: item.diagnostic.message.clone(),
            }
        })
        .collect::<Vec<_>>();
    let response = WorkerResponse {
        passed: report.artifact.is_some(),
        input_fingerprint,
        sources,
        input_set_complete,
        elapsed_ms: started.elapsed().as_millis() as u64,
        diagnostics,
        core_stage: report.dumps.core_stage.map(str::to_owned),
        core: report.dumps.core,
        cc_stage: report.dumps.cc_stage.map(str::to_owned),
        cc: report.dumps.cc,
        mir_stage: report.dumps.mir_stage.map(str::to_owned),
        mir: report.dumps.mir,
        trace: Some(trace),
    };
    atomic_write(
        Path::new(response_path),
        &serde_json::to_vec(&response).map_err(|error| error.to_string())?,
    )
}

pub(super) fn run_worker(
    work_root: &Path,
    index: usize,
    timeout_seconds: u64,
    request: WorkerRequest,
) -> Result<WorkerOutcome, String> {
    let worker_dir = work_root.join(format!("case-{index}"));
    let _ = fs::remove_dir_all(&worker_dir);
    fs::create_dir_all(&worker_dir)
        .map_err(|error| format!("{}: {error}", worker_dir.display()))?;
    let request_path = worker_dir.join("request.json");
    let response_path = worker_dir.join("response.json");
    let stderr_path = worker_dir.join("stderr.log");
    fs::write(
        &request_path,
        serde_json::to_vec(&request).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("{}: {error}", request_path.display()))?;
    let stderr = fs::File::create(&stderr_path)
        .map_err(|error| format!("{}: {error}", stderr_path.display()))?;
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let mut child = Command::new(executable)
        .arg("__diagnose-worker")
        .arg(&request_path)
        .arg(&response_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("could not start compiler worker: {error}"))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            let result = if response_path.is_file() {
                let bytes = fs::read(&response_path).map_err(|error| error.to_string())?;
                serde_json::from_slice(&bytes)
                    .map_err(|error| format!("worker returned invalid report: {error}"))?
            } else {
                let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
                let message = if stderr.trim().is_empty() {
                    format!("worker exited with {status} before writing a report")
                } else {
                    first_line(&stderr).to_owned()
                };
                return Ok(WorkerOutcome::Crashed {
                    message,
                    stderr,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
            };
            return Ok(WorkerOutcome::Completed(Box::new(result)));
        }
        if started.elapsed() >= Duration::from_secs(timeout_seconds) {
            child
                .kill()
                .map_err(|error| format!("could not stop timed-out worker: {error}"))?;
            let _ = child.wait();
            return Ok(WorkerOutcome::TimedOut {
                elapsed_ms: started.elapsed().as_millis() as u64,
            });
        }
        thread::sleep(Duration::from_millis(15));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn missing_entry_is_a_loading_error_without_fabricated_source() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after UNIX epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!("psrs-diagnose-worker-{unique}"));
        fs::create_dir_all(&root).expect("create worker test directory");
        let missing = root.join("missing.purs");
        let request_path = root.join("request.json");
        let response_path = root.join("response.json");
        let request = WorkerRequest {
            path: missing.to_string_lossy().into_owned(),
            category_dir: None,
            explicit_inputs: Vec::new(),
            capture_dumps: false,
            trusted_stdlib_fingerprint: String::new(),
        };
        fs::write(
            &request_path,
            serde_json::to_vec(&request).expect("serialize worker request"),
        )
        .expect("write worker request");

        worker(
            request_path.to_str().expect("request path is UTF-8"),
            response_path.to_str().expect("response path is UTF-8"),
        )
        .expect("worker writes loading-error response");
        let response: WorkerResponse =
            serde_json::from_slice(&fs::read(&response_path).expect("read worker response"))
                .expect("parse worker response");

        assert!(!response.passed);
        assert!(!response.input_set_complete);
        assert!(response.sources.is_empty());
        assert_eq!(response.diagnostics[0].stage, "input loading");
        assert!(response.diagnostics[0].message.contains("missing.purs"));
        let _ = fs::remove_dir_all(root);
    }
}

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{env, fs};

mod artifacts;
use artifacts::*;
mod report;
use report::*;
mod schema;
use schema::*;
mod trace;
use trace::*;
mod worker;
pub(super) use worker::worker;
use worker::{WorkerOutcome, WorkerRequest, WorkerResponse, run_worker};

const SCHEMA_VERSION: u32 = 2;
const DEFAULT_TIMEOUT: u64 = 20;

pub(super) fn run(args: Vec<String>) -> Result<(), String> {
    if args.first().is_some_and(|arg| arg == "--compare") {
        if args.len() != 3 {
            return Err(usage());
        }
        return compare(&args[1], &args[2]);
    }
    let options = Options::parse(args)?;
    let output = options.output.clone();
    let snapshot = diagnose(options)?;
    print_summary(&snapshot);
    write_snapshot(&output, &snapshot)?;
    println!("snapshot: {}", output.display());
    Ok(())
}

impl Options {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut file = None;
        let mut additional_inputs = Vec::new();
        let mut corpus = None;
        let mut filter = None;
        let mut limit = None;
        let mut timeout_seconds = DEFAULT_TIMEOUT;
        let mut output = PathBuf::from("diagnose.json");
        let mut trace = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--corpus" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err(usage());
                    };
                    corpus = Some(value.clone());
                }
                "--filter" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err(usage());
                    };
                    filter = Some(value.clone());
                }
                "--limit" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err(usage());
                    };
                    limit = Some(value.parse().map_err(|_| usage())?);
                }
                "--timeout" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err(usage());
                    };
                    timeout_seconds = value.parse().map_err(|_| usage())?;
                    if timeout_seconds == 0 {
                        return Err(usage());
                    }
                }
                "--out" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err(usage());
                    };
                    output = PathBuf::from(value);
                }
                "--input" => {
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err(usage());
                    };
                    additional_inputs.push(PathBuf::from(value));
                }
                "--trace" => trace = true,
                value if value.starts_with('-') => return Err(usage()),
                value => {
                    if file.replace(PathBuf::from(value)).is_some() {
                        return Err(usage());
                    }
                }
            }
            index += 1;
        }
        if file.is_some() == corpus.is_some()
            || corpus.as_deref().is_some_and(|name| name != "passing")
            || (corpus.is_some() && !additional_inputs.is_empty())
        {
            return Err(usage());
        }
        if let Some(main) = file.as_ref() {
            let mut seen = BTreeMap::new();
            for input in std::iter::once(main).chain(additional_inputs.iter()) {
                let identity = fs::canonicalize(input).unwrap_or_else(|_| input.clone());
                if seen.insert(identity, ()).is_some() {
                    return Err(format!("{}: duplicate diagnosis input", input.display()));
                }
            }
        }
        Ok(Self {
            file,
            additional_inputs,
            corpus,
            filter,
            limit,
            timeout_seconds,
            output,
            trace,
        })
    }
}

fn diagnose(options: Options) -> Result<Snapshot, String> {
    let corpus_root = options.corpus.as_ref().map(|_| corpus_root()).transpose()?;
    let mode = if options.file.is_some() {
        "file"
    } else {
        "corpus"
    };
    let mut selected = if let Some(file) = &options.file {
        vec![(file.clone(), None)]
    } else {
        let root = corpus_root.as_ref().expect("corpus mode has a root");
        let passing = root.join("passing");
        let mut paths = Vec::new();
        psrs_driver::collect_purs_files(&passing, &mut paths);
        paths.sort();
        paths
            .into_iter()
            .filter(|path| {
                options.filter.as_ref().is_none_or(|filter| {
                    path.strip_prefix(root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .contains(filter)
                })
            })
            .take(options.limit.unwrap_or(usize::MAX))
            .map(|path| (path, Some(passing.clone())))
            .collect()
    };
    if let Some(file) = &options.file {
        for input in std::iter::once(file).chain(options.additional_inputs.iter()) {
            if !input.is_file() {
                return Err(format!("{}: file does not exist", input.display()));
            }
        }
    }

    let output = absolute_path(&options.output)?;
    let bundles = output.with_extension("bundles");
    let work_root = env::temp_dir().join(format!("psrs-diagnose-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work_root);
    fs::create_dir_all(&work_root).map_err(|error| format!("{}: {error}", work_root.display()))?;
    let bundle_context = BundleContext {
        compiler: compiler_revision(),
        trusted_stdlib_fingerprint: trusted_stdlib_fingerprint()?,
        executable: env::var("PSRS_BIN")
            .ok()
            .or_else(|| {
                env::current_exe()
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "psrs".into()),
    };
    let mut records = Vec::with_capacity(selected.len());
    let total = selected.len();
    let mut worker_index = 0usize;
    for (path, category) in selected.drain(..) {
        let text =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let display_path = corpus_root
            .as_ref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        if options.corpus.is_some() && is_ffi_excluded(&path, &text) {
            records.push(CaseRecord {
                path: display_path,
                input_fingerprint: fingerprint_sources(&[SourceInput {
                    name: path.to_string_lossy().into_owned(),
                    text,
                }]),
                input_set_complete: false,
                elapsed_ms: 0,
                status: CaseStatus::Excluded,
                excluded_reason: Some("foreign import or adjacent JavaScript FFI file".into()),
                diagnostics: Vec::new(),
                first_blocker: None,
                bundle: None,
                trace: None,
            });
            eprintln!(
                "[{}/{}] {}: excluded (FFI)",
                worker_index + 1,
                total,
                path.display()
            );
            worker_index += 1;
            continue;
        }
        let bundle = bundles.join(format!("{:04}-{}", worker_index, safe_name(&display_path)));
        let request = WorkerRequest {
            path: path.to_string_lossy().into_owned(),
            category_dir: category
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            explicit_inputs: if category.is_none() && !options.additional_inputs.is_empty() {
                std::iter::once(&path)
                    .chain(options.additional_inputs.iter())
                    .map(|input| input.to_string_lossy().into_owned())
                    .collect()
            } else {
                Vec::new()
            },
            capture_dumps: options.trace,
            trusted_stdlib_fingerprint: bundle_context.trusted_stdlib_fingerprint.clone(),
        };
        let result = run_worker(&work_root, worker_index, options.timeout_seconds, request)?;
        let entry_source = vec![SourceInput {
            name: path.to_string_lossy().into_owned(),
            text,
        }];
        let (
            status,
            diagnostics,
            input_fingerprint,
            input_set_complete,
            elapsed_ms,
            blocker,
            bundle_path,
            trace_record,
        ) = match result {
            WorkerOutcome::Completed(response) => {
                let response = *response;
                let status = if response.passed {
                    CaseStatus::Passed
                } else {
                    CaseStatus::Failed
                };
                let blocker = response.diagnostics.first().map(first_blocker);
                let should_bundle = !response.passed || options.trace;
                if should_bundle {
                    write_bundle(
                        &bundle,
                        &response.sources,
                        &response,
                        &display_path,
                        response.input_set_complete,
                        &bundle_context,
                        options.trace,
                    )?;
                }
                (
                    status,
                    response.diagnostics,
                    response.input_fingerprint,
                    response.input_set_complete,
                    response.elapsed_ms,
                    blocker,
                    should_bundle.then(|| bundle.to_string_lossy().into_owned()),
                    response.trace,
                )
            }
            WorkerOutcome::TimedOut { elapsed_ms } => {
                write_empty_bundle(
                    &bundle,
                    &entry_source,
                    EmptyBundleFailure {
                        case: &display_path,
                        reason: "compiler worker timed out",
                        stderr: "",
                        input_set_complete: false,
                    },
                    &bundle_context,
                    options.trace,
                )?;
                (
                    CaseStatus::TimedOut,
                    Vec::new(),
                    fingerprint_sources(&entry_source),
                    false,
                    elapsed_ms,
                    Some(FirstBlocker {
                        stage: "timeout".into(),
                        category: "worker_timeout".into(),
                        message: format!("exceeded {}s", options.timeout_seconds),
                    }),
                    Some(bundle.to_string_lossy().into_owned()),
                    Some(empty_case_trace(
                        &entry_source,
                        false,
                        &fingerprint_sources(&entry_source),
                        options.trace,
                    )),
                )
            }
            WorkerOutcome::Crashed {
                message,
                stderr,
                elapsed_ms,
            } => {
                write_empty_bundle(
                    &bundle,
                    &entry_source,
                    EmptyBundleFailure {
                        case: &display_path,
                        reason: &message,
                        stderr: &stderr,
                        input_set_complete: false,
                    },
                    &bundle_context,
                    options.trace,
                )?;
                (
                    CaseStatus::Crashed,
                    Vec::new(),
                    fingerprint_sources(&entry_source),
                    false,
                    elapsed_ms,
                    Some(FirstBlocker {
                        stage: "worker".into(),
                        category: "worker_crash".into(),
                        message,
                    }),
                    Some(bundle.to_string_lossy().into_owned()),
                    Some(empty_case_trace(
                        &entry_source,
                        false,
                        &fingerprint_sources(&entry_source),
                        options.trace,
                    )),
                )
            }
        };
        eprintln!(
            "[{}/{}] {}: {:?} in {} ms",
            worker_index + 1,
            total,
            display_path,
            status,
            elapsed_ms
        );
        records.push(CaseRecord {
            path: display_path,
            input_fingerprint,
            input_set_complete,
            elapsed_ms,
            status,
            excluded_reason: None,
            diagnostics,
            first_blocker: blocker,
            bundle: bundle_path,
            trace: trace_record,
        });
        worker_index += 1;
    }
    let _ = fs::remove_dir_all(&work_root);
    let cohort = Cohort {
        mode: mode.into(),
        corpus: corpus_root.map(|root| root.to_string_lossy().into_owned()),
        filter: options.filter.clone(),
        limit: options.limit,
        timeout_seconds: options.timeout_seconds,
        trusted_stdlib_fingerprint: bundle_context.trusted_stdlib_fingerprint.clone(),
    };
    let mut snapshot = Snapshot {
        schema_version: SCHEMA_VERSION,
        cohort,
        compiler: bundle_context.compiler,
        environment: observed_environment(),
        trace_mode: if options.trace {
            TraceMode::Dumps
        } else {
            TraceMode::Manifest
        },
        cases: records,
        groups: Vec::new(),
        counts: BTreeMap::new(),
    };
    group_cases(&mut snapshot);
    if snapshot.cases.is_empty() {
        return Err("diagnosis selected no cases".into());
    }
    Ok(snapshot)
}

/// Reuse the diagnosis schema for the source side of a build/link report.
pub(super) fn build_lineage(
    sources: &[(String, String)],
    report: &psrs_driver::CompilationReport,
) -> Result<serde_json::Value, String> {
    let inputs = sources
        .iter()
        .map(|(name, text)| SourceInput {
            name: name.clone(),
            text: text.clone(),
        })
        .collect::<Vec<_>>();
    let trace = from_report(
        &inputs,
        true,
        &fingerprint_sources(&inputs),
        &trusted_stdlib_fingerprint()?,
        false,
        report,
    )?;
    let mut value = serde_json::to_value(&trace).map_err(|error| error.to_string())?;
    if let Some(artifact) = &report.artifact {
        let outputs = trace
            .artifacts
            .iter()
            .filter(|artifact| artifact.representation == "component_binary")
            .collect::<Vec<_>>();
        let [output] = outputs.as_slice() else {
            return Err("source build must produce exactly one component trace artifact".into());
        };
        value["component_output"] = serde_json::json!({
            "artifact": output.id, "sha256": psrs_linker::sha256_hex(&artifact.wasm),
        });
    }
    Ok(value)
}

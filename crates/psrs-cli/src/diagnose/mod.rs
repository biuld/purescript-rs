use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{env, fs};

mod artifacts;
use artifacts::*;
mod report;
use report::*;
mod worker;
pub(super) use worker::worker;
use worker::{WorkerOutcome, WorkerResponse, run_worker};

const SCHEMA_VERSION: u32 = 1;
const DEFAULT_TIMEOUT: u64 = 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SourceInput {
    name: String,
    text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DiagnosticRecord {
    origin: String,
    source: Option<String>,
    stage: String,
    start: u32,
    end: u32,
    code: Option<String>,
    kind: Option<String>,
    message: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CaseStatus {
    Passed,
    Failed,
    Excluded,
    TimedOut,
    Crashed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FirstBlocker {
    stage: String,
    category: String,
    message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CaseRecord {
    path: String,
    input_fingerprint: String,
    input_set_complete: bool,
    elapsed_ms: u64,
    status: CaseStatus,
    excluded_reason: Option<String>,
    diagnostics: Vec<DiagnosticRecord>,
    first_blocker: Option<FirstBlocker>,
    bundle: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GroupRecord {
    stage: String,
    category: String,
    sample_messages: Vec<String>,
    cases: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Cohort {
    mode: String,
    corpus: Option<String>,
    filter: Option<String>,
    limit: Option<usize>,
    timeout_seconds: u64,
    trusted_stdlib_fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CompilerRevision {
    head: Option<String>,
    dirty: bool,
    working_tree_fingerprint: String,
    binary_fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct BundleContext {
    compiler: CompilerRevision,
    trusted_stdlib_fingerprint: String,
    executable: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    schema_version: u32,
    cohort: Cohort,
    compiler: CompilerRevision,
    cases: Vec<CaseRecord>,
    groups: Vec<GroupRecord>,
    counts: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CompareRow {
    path: String,
    change: String,
    input_comparison: String,
    before: Option<String>,
    after: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CompareReport {
    compatible_cohort: bool,
    before_compiler: CompilerRevision,
    after_compiler: CompilerRevision,
    changes: Vec<CompareRow>,
}

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

struct Options {
    file: Option<PathBuf>,
    corpus: Option<String>,
    filter: Option<String>,
    limit: Option<usize>,
    timeout_seconds: u64,
    output: PathBuf,
}

impl Options {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut file = None;
        let mut corpus = None;
        let mut filter = None;
        let mut limit = None;
        let mut timeout_seconds = DEFAULT_TIMEOUT;
        let mut output = PathBuf::from("diagnose.json");
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
        {
            return Err(usage());
        }
        Ok(Self {
            file,
            corpus,
            filter,
            limit,
            timeout_seconds,
            output,
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
        if !file.is_file() {
            return Err(format!("{}: file does not exist", file.display()));
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
        let result = run_worker(
            &work_root,
            worker_index,
            &path,
            category.as_deref(),
            options.timeout_seconds,
        )?;
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
        ) = match result {
            WorkerOutcome::Completed(response) => {
                let status = if response.passed {
                    CaseStatus::Passed
                } else {
                    CaseStatus::Failed
                };
                let blocker = response.diagnostics.first().map(first_blocker);
                if !response.passed {
                    write_bundle(
                        &bundle,
                        &response.sources,
                        &response,
                        &display_path,
                        response.input_set_complete,
                        &bundle_context,
                    )?;
                }
                (
                    status,
                    response.diagnostics,
                    response.input_fingerprint,
                    response.input_set_complete,
                    response.elapsed_ms,
                    blocker,
                    (!response.passed).then(|| bundle.to_string_lossy().into_owned()),
                )
            }
            WorkerOutcome::TimedOut { elapsed_ms } => {
                write_empty_bundle(
                    &bundle,
                    &entry_source,
                    &display_path,
                    "compiler worker timed out",
                    "",
                    false,
                    &bundle_context,
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
                    &display_path,
                    &message,
                    &stderr,
                    false,
                    &bundle_context,
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

//! Source compilation with optional explicit guest composition and joined lineage.
use super::*;

pub(super) fn run(command: &str, raw_args: Vec<String>) -> Result<(), String> {
    let mut paths = Vec::new();
    let mut output_path = None;
    let mut manifest_path = None;
    let mut report_path = None;
    let mut index = 0;
    while index < raw_args.len() {
        match raw_args[index].as_str() {
            "--manifest" | "--report" if command == "build" => {
                let slot = if raw_args[index] == "--manifest" {
                    &mut manifest_path
                } else {
                    &mut report_path
                };
                if slot.is_some() {
                    return Err(usage());
                }
                *slot = Some(raw_args.get(index + 1).ok_or_else(usage)?.clone());
                index += 2;
            }
            "-o" => {
                if output_path.is_some() {
                    return Err(usage());
                }
                let Some(output) = raw_args.get(index + 1) else {
                    return Err(usage());
                };
                output_path = Some(output.clone());
                index += 2;
            }
            path if path.starts_with('-') => return Err(usage()),
            path => {
                paths.push(path.to_owned());
                index += 1;
            }
        }
    }
    if paths.is_empty() {
        return Err(usage());
    }

    let sources = psrs_driver::load_program_files(&paths)?;
    let inputs = sources
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let mut source_lineage = None;
    let compilation = if report_path.is_some() {
        let report = psrs_driver::compile_program_sources_with_prelude_diagnosis(&inputs, false);
        source_lineage = Some(diagnose::build_lineage(&sources, &report)?);
        report.artifact.ok_or(report.diagnostics)
    } else {
        psrs_driver::compile_program_sources_with_prelude(&inputs)
    };
    let artifact = match compilation {
        Ok(artifact) => artifact,
        Err(errors) => {
            for error in errors {
                // A diagnostic in a standard-library module names a file the
                // caller did not pass, so there is no snippet to print, and
                // naming the first source instead would blame a file the caller
                // wrote for the library's error.
                let Some((path, text)) = error
                    .source
                    .source_index()
                    .and_then(|source| sources.get(source))
                else {
                    eprintln!(
                        "{}: {} [{}]: {}",
                        match error.source {
                            psrs_driver::DiagnosticOrigin::Library => "standard library",
                            _ => "program",
                        },
                        error.diagnostic.message,
                        error.diagnostic.stage,
                        error.diagnostic.code.unwrap_or("no error code")
                    );
                    continue;
                };
                let source = SourceFile::new(path.as_str(), text.as_str());
                print_coded_diagnostic(
                    &source,
                    error.diagnostic.span,
                    error.diagnostic.stage,
                    error.diagnostic.code,
                    &error.diagnostic.message,
                );
            }
            return Err(String::new());
        }
    };
    print_warnings(&artifact.warnings, &sources);
    if command == "wat" {
        if let Some(output) = output_path {
            fs::write(&output, artifact.wat).map_err(|error| format!("{output}: {error}"))?;
        } else {
            print!("{}", artifact.wat);
        }
    } else {
        let output = output_path.unwrap_or_else(|| {
            std::path::Path::new(&paths[0])
                .with_extension("wasm")
                .to_string_lossy()
                .into_owned()
        });
        let application_sha256 = psrs_linker::sha256_hex(&artifact.wasm);
        let (bytes, composition) = if let Some(manifest) = manifest_path {
            match link::compose(&artifact.wasm, &manifest, false) {
                Ok((bytes, evidence)) => (bytes, Some(evidence)),
                Err(error) => {
                    if let Some(path) = &report_path {
                        link::write_report(
                            path,
                            &serde_json::json!({
                                "schema_version": 1, "status": "rejected",
                                "source_compilation": source_lineage,
                                "application_sha256": application_sha256,
                                "composition": { "status": "rejected", "diagnostic": error },
                            }),
                        )?;
                    }
                    return Err(error);
                }
            }
        } else {
            (artifact.wasm, None)
        };
        if let Some(path) = report_path {
            link::write_report(
                &path,
                &serde_json::json!({
                    "schema_version": 1, "status": "completed", "source_compilation": source_lineage,
                    "application_sha256": application_sha256, "composition": composition,
                    "output_sha256": psrs_linker::sha256_hex(&bytes),
                }),
            )?;
        }
        fs::write(&output, bytes).map_err(|error| format!("{output}: {error}"))?;
        println!("wrote {output}");
    }
    Ok(())
}

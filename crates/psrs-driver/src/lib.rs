use psrs_span::{SourceFile, TextRange};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub stage: &'static str,
    pub span: TextRange,
    pub message: String,
    /// The official PureScript `errorCode` for this diagnostic, when it maps to
    /// one. Internal or unsupported-syntax diagnostics have no code.
    pub code: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub wasm: Vec<u8>,
    pub wat: String,
}

/// A diagnostic attributed to one source in a multi-module program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramDiagnostic {
    /// Index into the source list passed to [`resolve_program_sources`].
    pub source: usize,
    pub diagnostic: Diagnostic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrDumps {
    pub core: String,
    pub cc: String,
    pub mir: String,
}

impl IrDumps {
    pub fn get(&self, stage: &str) -> Option<&str> {
        match stage {
            "core" => Some(&self.core),
            "cc" => Some(&self.cc),
            "mir" => Some(&self.mir),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compilation {
    pub artifact: Artifact,
    pub dumps: IrDumps,
}

pub fn compile_source(source_name: &str, source_text: &str) -> Result<Artifact, Vec<Diagnostic>> {
    let core = lower_source_to_core(source_name, source_text)?;
    let output = psrs_backend::compile(core).map_err(backend_diagnostics)?;
    Ok(Artifact {
        wasm: output.wasm,
        wat: output.wat,
    })
}

pub fn compile_source_with_dumps(
    source_name: &str,
    source_text: &str,
) -> Result<Compilation, Vec<Diagnostic>> {
    let core = lower_source_to_core(source_name, source_text)?;
    let core_dump = format!("{core:#?}");
    let stages = psrs_backend::compile_with_stages(core).map_err(backend_diagnostics)?;
    Ok(Compilation {
        artifact: Artifact {
            wasm: stages.artifact.wasm,
            wat: stages.artifact.wat,
        },
        dumps: IrDumps {
            core: core_dump,
            cc: format!("{:#?}", stages.cc),
            mir: format!("{:#?}", stages.mir),
        },
    })
}

fn check_source_to_thir(
    source_name: &str,
    source_text: &str,
) -> Result<psrs_thir::Module, Vec<Diagnostic>> {
    let ast = lower_source_to_ast(source_name, source_text)?;
    let intrinsics = psrs_resolve::bootstrap_externals();
    let hir = psrs_resolve::resolve_module_with_externals(ast, psrs_hir::ModuleId(0), &intrinsics)
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| {
                    coded_diagnostic(
                        "P3 resolve",
                        error.span,
                        error.error_code(),
                        error.message(),
                    )
                })
                .collect::<Vec<_>>()
        })?;
    let hir = psrs_desugar::desugar_module(hir).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| diagnostic("P4 desugar", error.span, error.message))
            .collect::<Vec<_>>()
    })?;
    let thir = psrs_typecheck::typecheck_module(hir).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| diagnostic("P5 typecheck", error.span, error.message()))
            .collect::<Vec<_>>()
    })?;
    Ok(thir)
}

/// Runs lexing, layout, parsing, and surface lowering (P0–P2), returning the
/// unresolved AST module.
fn lower_source_to_ast(
    source_name: &str,
    source_text: &str,
) -> Result<psrs_ast::Module, Vec<Diagnostic>> {
    let source = SourceFile::new(source_name, source_text);
    let (tokens, lex_errors) = psrs_syntax::lex(source.text());
    if !lex_errors.is_empty() {
        return Err(lex_errors
            .into_iter()
            .map(|error| {
                coded_diagnostic(
                    "P0 lex",
                    error.span,
                    Some("ErrorParsingModule"),
                    error.message,
                )
            })
            .collect());
    }
    let layout_tokens = psrs_syntax::add_layout(&source, &tokens);
    let cst = psrs_syntax::parse_module(&layout_tokens).map_err(|error| {
        vec![coded_diagnostic(
            "P1 parse",
            error.span,
            Some("ErrorParsingModule"),
            error.message,
        )]
    })?;
    psrs_ast::lower_module(cst).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| {
                coded_diagnostic("P2 surface lowering", error.span, error.code, error.message)
            })
            .collect::<Vec<_>>()
    })
}

/// Resolves a program from a list of `(source_name, source_text)` pairs. Module
/// IDs equal each source's index in the list. This is the module-graph entry
/// point used to measure module, import, export, and name resolution.
pub fn resolve_program_sources(
    sources: &[(&str, &str)],
) -> Result<Vec<psrs_hir::Module>, Vec<ProgramDiagnostic>> {
    let mut modules = Vec::with_capacity(sources.len());
    let mut errors = Vec::new();
    for (index, (name, text)) in sources.iter().enumerate() {
        match lower_source_to_ast(name, text) {
            Ok(module) => modules.push(module),
            Err(diagnostics) => {
                for diagnostic in diagnostics {
                    errors.push(ProgramDiagnostic {
                        source: index,
                        diagnostic,
                    });
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    match psrs_resolve::resolve_program(modules) {
        Ok(resolved) => Ok(resolved),
        Err(program_errors) => Err(program_errors
            .into_iter()
            .map(|error| ProgramDiagnostic {
                source: error.module,
                diagnostic: coded_diagnostic(
                    "P3 resolve",
                    error.error.span,
                    error.error.error_code(),
                    error.error.message(),
                ),
            })
            .collect()),
    }
}

/// Resolves a program and reports only resolution diagnostics. Type checking a
/// multi-module program is not implemented yet.
pub fn check_program(sources: &[(&str, &str)]) -> Result<(), Vec<ProgramDiagnostic>> {
    resolve_program_sources(sources).map(|_| ())
}

/// Resolves a program while tolerating imports whose modules are not provided,
/// and reports every resolution diagnostic. This is used to measure module,
/// import, export, and name resolution against the corpus, where support
/// libraries such as `Prelude` are not part of the input.
pub fn check_program_lenient(sources: &[(&str, &str)]) -> Result<(), Vec<ProgramDiagnostic>> {
    let mut modules = Vec::with_capacity(sources.len());
    let mut errors = Vec::new();
    for (index, (name, text)) in sources.iter().enumerate() {
        match lower_source_to_ast(name, text) {
            Ok(module) => modules.push(module),
            Err(diagnostics) => {
                for diagnostic in diagnostics {
                    errors.push(ProgramDiagnostic {
                        source: index,
                        diagnostic,
                    });
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let options = psrs_resolve::ResolveOptions {
        tolerate_missing_modules: true,
    };
    match psrs_resolve::resolve_program_with_options(modules, options) {
        Ok(_) => Ok(()),
        Err(program_errors) => Err(program_errors
            .into_iter()
            .map(|error| ProgramDiagnostic {
                source: error.module,
                diagnostic: coded_diagnostic(
                    "P3 resolve",
                    error.error.span,
                    error.error.error_code(),
                    error.error.message(),
                ),
            })
            .collect()),
    }
}

/// Runs the source stages P0 through P5 and reports diagnostics without
/// lowering to Core or the backend. Useful for checking source acceptance.
pub fn check_source(source_name: &str, source_text: &str) -> Result<(), Vec<Diagnostic>> {
    check_source_to_thir(source_name, source_text).map(|_| ())
}

/// Runs lexing, layout, and parsing only (P0–P2). Reports diagnostics and
/// never reaches name resolution, type checking, or the backend.
pub fn parse_source(source_name: &str, source_text: &str) -> Result<(), Vec<Diagnostic>> {
    let source = SourceFile::new(source_name, source_text);
    let (tokens, lex_errors) = psrs_syntax::lex(source.text());
    if !lex_errors.is_empty() {
        return Err(lex_errors
            .into_iter()
            .map(|error| {
                coded_diagnostic(
                    "P0 lex",
                    error.span,
                    Some("ErrorParsingModule"),
                    error.message,
                )
            })
            .collect());
    }
    let layout_tokens = psrs_syntax::add_layout(&source, &tokens);
    psrs_syntax::parse_module(&layout_tokens).map_err(|error| {
        vec![coded_diagnostic(
            "P1 parse",
            error.span,
            Some("ErrorParsingModule"),
            error.message,
        )]
    })?;
    Ok(())
}

fn lower_source_to_core(
    source_name: &str,
    source_text: &str,
) -> Result<psrs_core::Module, Vec<Diagnostic>> {
    let thir = check_source_to_thir(source_name, source_text)?;
    let core = psrs_core::lower_module(thir).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| diagnostic("P6 Core lowering", error.span, error.message))
            .collect::<Vec<_>>()
    })?;
    Ok(core)
}

fn backend_diagnostics(errors: Vec<psrs_backend::BackendError>) -> Vec<Diagnostic> {
    errors
        .into_iter()
        .map(|error| diagnostic(error.pass, error.span, error.message))
        .collect()
}

fn diagnostic(stage: &'static str, span: TextRange, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        stage,
        span,
        message: message.into(),
        code: None,
    }
}

fn coded_diagnostic(
    stage: &'static str,
    span: TextRange,
    code: Option<&'static str>,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        stage,
        span,
        message: message.into(),
        code,
    }
}

#[cfg(test)]
mod tests;

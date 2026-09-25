use psrs_span::{SourceFile, TextRange};

mod prelude;
mod program;

pub use program::{
    check_program, check_program_kinds_lenient, check_program_lenient, compile_program_sources,
    compile_program_sources_with_prelude, resolve_program_sources, typecheck_program_sources,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub stage: &'static str,
    pub span: TextRange,
    pub message: String,
    /// The official PureScript `errorCode` for this diagnostic, when it maps to
    /// one. Internal or unsupported-syntax diagnostics have no code.
    pub code: Option<&'static str>,
    /// The backend error category, when this diagnostic comes from the backend.
    /// It distinguishes invalid compiler IR from unsupported source input.
    pub kind: Option<psrs_backend::BackendErrorKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub wasm: Vec<u8>,
    pub wat: String,
    pub warnings: Vec<Warning>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// Index into the source list passed to the compile function. Single-file
    /// compilation uses source index zero.
    pub source: usize,
    pub diagnostic: Diagnostic,
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
    let core = lower_source_with_prelude_to_core(source_name, source_text)?;
    let output = psrs_backend::compile(core).map_err(backend_diagnostics)?;
    let warnings = backend_warnings(output.warnings, prelude::SOURCES.len());
    Ok(Artifact {
        wasm: output.wasm,
        wat: output.wat,
        warnings,
    })
}

/// Parses the standard library and a program module and links both into one
/// Core module. Library declarations the program does not reach are pruned.
fn lower_source_with_prelude_to_core(
    source_name: &str,
    source_text: &str,
) -> Result<psrs_core::Module, Vec<Diagnostic>> {
    let mut sources = Vec::with_capacity(prelude::SOURCES.len() + 1);
    sources.extend_from_slice(prelude::SOURCES);
    sources.push((source_name, source_text));
    program::lower_program_to_core_with_trusted_prefix(&sources, prelude::SOURCES.len())
        .map_err(|errors| program_diagnostics_from_hidden_prelude(errors, prelude::SOURCES.len()))
}

fn program_diagnostics_from_hidden_prelude(
    errors: Vec<ProgramDiagnostic>,
    hidden_sources: usize,
) -> Vec<Diagnostic> {
    errors
        .into_iter()
        .map(|mut error| {
            error.source = error.source.saturating_sub(hidden_sources);
            error
        })
        .map(|error| error.diagnostic)
        .collect()
}

#[cfg(test)]
pub(crate) fn lower_source_to_core(
    source_name: &str,
    source_text: &str,
) -> Result<psrs_core::Module, Vec<Diagnostic>> {
    lower_source_with_prelude_to_core(source_name, source_text)
}

pub fn compile_source_with_dumps(
    source_name: &str,
    source_text: &str,
) -> Result<Compilation, Vec<Diagnostic>> {
    let core = lower_source_with_prelude_to_core(source_name, source_text)?;
    let stages = psrs_backend::compile_with_stages(core).map_err(backend_diagnostics)?;
    let warnings = backend_warnings(stages.artifact.warnings, prelude::SOURCES.len());
    Ok(Compilation {
        artifact: Artifact {
            wasm: stages.artifact.wasm,
            wat: stages.artifact.wat,
            warnings,
        },
        dumps: IrDumps {
            core: format!("{:#?}", stages.core),
            cc: format!("{:#?}", stages.cc),
            mir: format!("{:#?}", stages.mir),
        },
    })
}

/// Runs lexing, layout, parsing, and surface lowering (P0–P2), returning the
/// unresolved AST module.
pub(crate) fn lower_source_to_ast(
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

/// Runs the source stages P0 through P5 and reports diagnostics without
/// lowering to Core or the backend. Useful for checking source acceptance.
pub fn check_source(source_name: &str, source_text: &str) -> Result<(), Vec<Diagnostic>> {
    let mut sources = Vec::with_capacity(prelude::SOURCES.len() + 1);
    sources.extend_from_slice(prelude::SOURCES);
    sources.push((source_name, source_text));
    program::check_program_with_trusted_prefix(&sources, prelude::SOURCES.len())
        .map_err(|errors| program_diagnostics_from_hidden_prelude(errors, prelude::SOURCES.len()))
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

fn backend_diagnostics(errors: Vec<psrs_backend::BackendError>) -> Vec<Diagnostic> {
    errors
        .into_iter()
        .map(|error| Diagnostic {
            stage: error.pass,
            span: error.span,
            message: error.message,
            code: None,
            kind: Some(error.kind),
        })
        .collect()
}

pub(crate) fn backend_warnings(
    warnings: Vec<psrs_backend::BackendWarning>,
    hidden_sources: usize,
) -> Vec<Warning> {
    warnings
        .into_iter()
        .filter_map(|warning| {
            let source = warning
                .module
                .map_or(hidden_sources, |module| module.0 as usize);
            (source >= hidden_sources).then(|| Warning {
                source: source - hidden_sources,
                diagnostic: diagnostic(warning.pass, warning.span, warning.message),
            })
        })
        .collect()
}

fn diagnostic(stage: &'static str, span: TextRange, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        stage,
        span,
        message: message.into(),
        code: None,
        kind: None,
    }
}

pub(crate) fn coded_diagnostic(
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
        kind: None,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests/parameterized_shapes.rs"]
mod parameterized_shapes;

#[cfg(test)]
#[path = "tests/generic_aggregate_audit.rs"]
mod generic_aggregate_audit;

#[cfg(test)]
#[path = "tests/polymorphism_erasure_audit.rs"]
mod polymorphism_erasure_audit;

#[cfg(test)]
#[path = "tests/cc_ir_audit.rs"]
mod cc_ir_audit;

use psrs_span::{SourceFile, TextRange};

mod prelude;
mod program;

pub use program::{
    check_program, check_program_kinds_lenient, check_program_lenient, compile_program_sources,
    resolve_program_sources, typecheck_program_sources,
};

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
    let core = lower_source_with_prelude_to_core(source_name, source_text)?;
    let output = psrs_backend::compile(core).map_err(backend_diagnostics)?;
    Ok(Artifact {
        wasm: output.wasm,
        wat: output.wat,
    })
}

/// Parses the standard library and a program module and links both into one
/// Core module. Library declarations the program does not reach are pruned.
fn lower_source_with_prelude_to_core(
    source_name: &str,
    source_text: &str,
) -> Result<psrs_core::Module, Vec<Diagnostic>> {
    let sources = [(prelude::NAME, prelude::SOURCE), (source_name, source_text)];
    program::lower_program_to_core(&sources).map_err(program_diagnostics)
}

fn program_diagnostics(errors: Vec<ProgramDiagnostic>) -> Vec<Diagnostic> {
    errors.into_iter().map(|error| error.diagnostic).collect()
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
    let sources = [(prelude::NAME, prelude::SOURCE), (source_name, source_text)];
    program::check_program(&sources).map_err(program_diagnostics)
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
    }
}

#[cfg(test)]
mod tests;

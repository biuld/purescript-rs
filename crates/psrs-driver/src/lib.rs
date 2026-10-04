use psrs_span::{SourceFile, TextRange};

mod loader;
mod prelude;
mod program;

pub use loader::load_program_files;
pub use program::{
    check_program, check_program_kinds_lenient, check_program_kinds_lenient_with_prelude,
    check_program_lenient, check_program_lenient_with_prelude, check_program_types_lenient,
    check_program_types_lenient_with_prelude, check_program_with_warnings, compile_program_sources,
    compile_program_sources_with_prelude, resolve_program_sources, typecheck_program_sources,
    typecheck_program_sources_with_warnings,
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

/// Where a diagnostic in a multi-module program came from.
///
/// A diagnostic is not always attributable to one source. The backend reports
/// some failures against the program as a whole, such as a component with no
/// entry point, and an entry point that prepends the standard library reports
/// failures inside a module the caller never passed. Both cases need to stay
/// distinguishable from a real source index, or a caller reads them as a
/// diagnostic against a file it owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticOrigin {
    /// The source at this index in the source list passed to the entry point.
    Source(usize),
    /// A module of the trusted standard library an entry point prepends. The
    /// library is not part of the caller's source list, so it has no index there.
    Library,
    /// The program as a whole, with no single module at fault.
    Program,
}

impl DiagnosticOrigin {
    /// The index of a source the caller passed, or [`None`] for a diagnostic
    /// that names no source the caller owns.
    pub fn source_index(self) -> Option<usize> {
        match self {
            Self::Source(index) => Some(index),
            Self::Library | Self::Program => None,
        }
    }
}

/// A diagnostic attributed to one source in a multi-module program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramDiagnostic {
    pub source: DiagnosticOrigin,
    pub diagnostic: Diagnostic,
}

/// A warning attributed to one module in a multi-module program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramWarning {
    pub source: DiagnosticOrigin,
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
    let lowered = lower_source_with_prelude_to_core(source_name, source_text)?;
    let mut warnings = lowered.warnings;
    let output = psrs_backend::compile_with_context(
        lowered.core,
        lowered.effect_context,
        psrs_backend::TargetCapabilities::default(),
    )
    .map_err(backend_diagnostics)?
    .artifact;
    warnings.extend(backend_warnings(output.warnings, lowered.trusted_prefix));
    Ok(Artifact {
        wasm: output.wasm,
        wat: output.wat,
        warnings,
    })
}

struct LoweredSource {
    core: psrs_core::Module,
    trusted_prefix: usize,
    warnings: Vec<Warning>,
    effect_context: Option<psrs_core::effect::EffectCompilation>,
}

/// Parses the standard library and a program module and links both into one
/// Core module. Library declarations the program does not reach are pruned.
fn lower_source_with_prelude_to_core(
    source_name: &str,
    source_text: &str,
) -> Result<LoweredSource, Vec<Diagnostic>> {
    let (sources, trusted_prefix) = prepend_stdlib(&[(source_name, source_text)])?;
    let (core, warnings, effect_context) =
        program::lower_program_to_core_and_effect_context(&sources, trusted_prefix)
            .map_err(program_diagnostics_from_hidden_prelude)?;
    Ok(LoweredSource {
        core,
        trusted_prefix,
        warnings: program::typecheck_warnings(warnings, trusted_prefix),
        effect_context,
    })
}

fn prepend_stdlib<'a>(
    sources: &[(&'a str, &'a str)],
) -> Result<prelude::PrefixedSources<'a>, Vec<Diagnostic>> {
    prelude::prepend(sources).map_err(|message| {
        vec![diagnostic(
            "stdlib",
            psrs_span::TextRange::default(),
            message,
        )]
    })
}

/// Drops the source index from diagnostics reported against a hidden standard
/// library prefix.
///
/// These entry points return a bare [`Diagnostic`], which has no source index, so
/// there is nothing to rebase: the index is discarded rather than clamped onto a
/// caller source that does not exist.
fn program_diagnostics_from_hidden_prelude(errors: Vec<ProgramDiagnostic>) -> Vec<Diagnostic> {
    errors.into_iter().map(|error| error.diagnostic).collect()
}

#[cfg(test)]
pub(crate) fn lower_source_to_core(
    source_name: &str,
    source_text: &str,
) -> Result<psrs_core::Module, Vec<Diagnostic>> {
    lower_source_with_prelude_to_core(source_name, source_text).map(|lowered| lowered.core)
}

/// Linked Core plus the trusted Effect contract required to lower it.
#[cfg(test)]
pub(crate) struct PreparedSource {
    pub core: psrs_core::Module,
    pub effect_context: Option<psrs_core::effect::EffectCompilation>,
}

#[cfg(test)]
pub(crate) fn prepare_main(source: &str) -> Result<PreparedSource, Vec<Diagnostic>> {
    prepare_sources(&[("Main.purs", source)])
}

#[cfg(test)]
pub(crate) fn prepare_sources(sources: &[(&str, &str)]) -> Result<PreparedSource, Vec<Diagnostic>> {
    let (sources, trusted_prefix) = prepend_stdlib(sources)?;
    let (core, _warnings, effect_context) =
        program::lower_program_to_core_and_effect_context(&sources, trusted_prefix)
            .map_err(program_diagnostics_from_hidden_prelude)?;
    Ok(PreparedSource {
        core,
        effect_context,
    })
}

#[cfg(test)]
pub(crate) fn compile_main_stages(source: &str) -> Result<psrs_backend::Stages, Vec<Diagnostic>> {
    compile_main_with_target(source, psrs_backend::TargetCapabilities::default())
}

#[cfg(test)]
pub(crate) fn compile_main_with_target(
    source: &str,
    target: psrs_backend::TargetCapabilities,
) -> Result<psrs_backend::Stages, Vec<Diagnostic>> {
    let prepared = prepare_main(source)?;
    psrs_backend::compile_with_context(prepared.core, prepared.effect_context, target)
        .map_err(backend_diagnostics)
}

#[cfg(test)]
pub(crate) fn lower_main_to_cc(
    source: &str,
) -> Result<psrs_backend::BackendInput, Vec<Diagnostic>> {
    let prepared = prepare_main(source)?;
    psrs_backend::lower_cc_with_context(prepared.core, prepared.effect_context.as_ref())
        .map_err(backend_diagnostics)
}

pub fn compile_source_with_dumps(
    source_name: &str,
    source_text: &str,
) -> Result<Compilation, Vec<Diagnostic>> {
    let lowered = lower_source_with_prelude_to_core(source_name, source_text)?;
    let mut warnings = lowered.warnings;
    let stages = psrs_backend::compile_with_context(
        lowered.core,
        lowered.effect_context,
        psrs_backend::TargetCapabilities::default(),
    )
    .map_err(backend_diagnostics)?;
    warnings.extend(backend_warnings(
        stages.artifact.warnings,
        lowered.trusted_prefix,
    ));
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
    check_source_with_warnings(source_name, source_text).map(|_| ())
}

/// Checks a source and returns warnings emitted by successful type checking.
pub fn check_source_with_warnings(
    source_name: &str,
    source_text: &str,
) -> Result<Vec<Warning>, Vec<Diagnostic>> {
    let (sources, trusted_prefix) = prepend_stdlib(&[(source_name, source_text)])?;
    let warnings =
        program::check_program_with_trusted_prefix_and_warnings(&sources, trusted_prefix)
            .map_err(program_diagnostics_from_hidden_prelude)?;
    Ok(program::typecheck_warnings(warnings, trusted_prefix))
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

#[cfg(test)]
#[path = "tests/dictionary_audit/mod.rs"]
mod dictionary_audit;

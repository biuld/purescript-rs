use psrs_span::{SourceFile, TextRange};
use std::collections::HashSet;

mod prelude;

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
    let mut ast = lower_source_to_ast(source_name, source_text)?;
    merge_prelude(&mut ast)?;
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
    let kind_errors = psrs_kind::check_module(&hir);
    if !kind_errors.is_empty() {
        return Err(kind_errors
            .into_iter()
            .map(|error| {
                coded_diagnostic("P5 kind check", error.span, Some(error.code), error.message)
            })
            .collect());
    }
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

/// Parses the embedded standard library and appends the declarations the
/// program reaches, together with every library `foreign import`. The bootstrap
/// compiler has no module loader or linker, so the library is combined with the
/// program before resolution; see [`prelude`]. Only reachable declarations are
/// added so an unused library function does not pull in its WIT imports.
fn merge_prelude(module: &mut psrs_ast::Module) -> Result<(), Vec<Diagnostic>> {
    let mut prelude = lower_source_to_ast("<prelude>", prelude::SOURCE)?;
    let mut needed = HashSet::new();
    for declaration in &module.declarations {
        collect_names(&declaration.value, &mut needed);
    }
    let mut include = vec![false; prelude.declarations.len()];
    loop {
        let mut changed = false;
        for (index, declaration) in prelude.declarations.iter().enumerate() {
            if include[index] || !needed.contains(&declaration.name.text) {
                continue;
            }
            include[index] = true;
            changed = true;
            collect_names(&declaration.value, &mut needed);
        }
        if !changed {
            break;
        }
    }
    for (index, declaration) in std::mem::take(&mut prelude.declarations)
        .into_iter()
        .enumerate()
    {
        if include[index] {
            module.declarations.push(declaration);
        }
    }
    module.foreign_imports.append(&mut prelude.foreign_imports);
    Ok(())
}

/// Collects every value name an expression mentions. Used only to decide which
/// standard-library declarations a program reaches; over-approximation is safe.
fn collect_names(expression: &psrs_ast::Expr, names: &mut HashSet<String>) {
    use psrs_ast::ExprKind;
    match &expression.kind {
        ExprKind::Name(name) => {
            names.insert(name.text.clone());
        }
        ExprKind::Integer(_) | ExprKind::String(_) | ExprKind::Char(_) => {}
        ExprKind::Application(function, argument) => {
            collect_names(function, names);
            collect_names(argument, names);
        }
        ExprKind::Operator {
            operator,
            left,
            right,
        } => {
            names.insert(operator.text.clone());
            collect_names(left, names);
            collect_names(right, names);
        }
        ExprKind::Lambda { body, .. } => collect_names(body, names),
        ExprKind::Let { declarations, body } => {
            for declaration in declarations {
                collect_names(&declaration.value, names);
            }
            collect_names(body, names);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_names(condition, names);
            collect_names(then_branch, names);
            collect_names(else_branch, names);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            collect_names(scrutinee, names);
            for branch in branches {
                collect_names(&branch.value, names);
            }
        }
    }
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
    resolve_program_lenient(sources).map(|_| ())
}

/// Resolves a lenient program and then kind-checks every module that resolved.
/// Missing support libraries still produce resolution diagnostics, but a module
/// that resolves without them is kind-checked, so the M3 layer is measurable
/// against the corpus.
pub fn check_program_kinds_lenient(sources: &[(&str, &str)]) -> Result<(), Vec<ProgramDiagnostic>> {
    let mut modules = lower_program_to_ast(sources)?;
    let options = psrs_resolve::ResolveOptions {
        tolerate_missing_modules: true,
    };
    let (resolved, program_errors) =
        psrs_resolve::resolve_program_partial(std::mem::take(&mut modules), options);
    let mut errors = Vec::new();
    for error in program_errors {
        errors.push(ProgramDiagnostic {
            source: error.module,
            diagnostic: coded_diagnostic(
                "P3 resolve",
                error.error.span,
                error.error.error_code(),
                error.error.message(),
            ),
        });
    }
    for (source, module) in resolved.iter().enumerate() {
        let Some(module) = module else {
            continue;
        };
        for error in psrs_kind::check_module(module) {
            errors.push(ProgramDiagnostic {
                source,
                diagnostic: coded_diagnostic(
                    "P5 kind check",
                    error.span,
                    Some(error.code),
                    error.message,
                ),
            });
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn resolve_program_lenient(
    sources: &[(&str, &str)],
) -> Result<Vec<psrs_hir::Module>, Vec<ProgramDiagnostic>> {
    let modules = lower_program_to_ast(sources)?;
    let options = psrs_resolve::ResolveOptions {
        tolerate_missing_modules: true,
    };
    match psrs_resolve::resolve_program_with_options(modules, options) {
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

fn lower_program_to_ast(
    sources: &[(&str, &str)],
) -> Result<Vec<psrs_ast::Module>, Vec<ProgramDiagnostic>> {
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
    if errors.is_empty() {
        Ok(modules)
    } else {
        Err(errors)
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

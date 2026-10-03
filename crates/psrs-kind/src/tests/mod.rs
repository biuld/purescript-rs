//! Kind checking cases. The helpers here resolve a source the way the driver
//! does; the cases are grouped by the rule each one exercises.

use crate::check_program;
use crate::kind::KindDiagnostic;
use psrs_hir::ModuleId;

mod declarations;
mod instances;
mod program;
mod roles;

pub(super) fn check(source: &str) -> Vec<KindDiagnostic> {
    check_modules(&[resolve(source)])
}

/// Kind checks a program of resolved modules the way the driver does.
pub(super) fn check_modules(modules: &[psrs_hir::Module]) -> Vec<KindDiagnostic> {
    let (_, diagnostics) = check_program(modules);
    diagnostics
}

pub(super) fn check_env(
    modules: &[psrs_hir::Module],
) -> (crate::CheckedKindEnv, Vec<KindDiagnostic>) {
    check_program(modules)
}

fn resolve(source: &str) -> psrs_hir::Module {
    let module = lower("Test.purs", source);
    let intrinsics = psrs_resolve::bootstrap_externals();
    psrs_resolve::resolve_module_with_externals(module, ModuleId(0), &intrinsics).expect("resolve")
}

fn lower(name: &str, source: &str) -> psrs_ast::Module {
    let file = psrs_span::SourceFile::new(name, source);
    let (tokens, errors) = psrs_syntax::lex(file.text());
    assert!(errors.is_empty(), "lex errors: {errors:?}");
    let layout = psrs_syntax::add_layout(&file, &tokens);
    let cst = psrs_syntax::parse_module(&layout).expect("parse");
    psrs_ast::lower_module(cst).expect("surface lowering")
}

/// Resolves a program whose module IDs are the index of each source, the way
/// `resolve_program_sources` assigns them.
pub(super) fn resolve_program(sources: &[(&str, &str)]) -> Vec<psrs_hir::Module> {
    let modules = sources
        .iter()
        .map(|(name, source)| lower(name, source))
        .collect();
    psrs_resolve::resolve_program(modules).expect("resolve")
}

pub(super) fn check_ok(source: &str) {
    let errors = check(source);
    assert!(errors.is_empty(), "unexpected kind errors: {errors:?}");
}

pub(super) fn codes(errors: &[KindDiagnostic]) -> Vec<&'static str> {
    errors.iter().map(|error| error.code).collect()
}

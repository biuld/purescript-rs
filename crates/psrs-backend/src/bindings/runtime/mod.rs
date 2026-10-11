//! Checked source contracts for ordinary runtime provider bindings.
//! Export lookup and physical operations belong to psrs-runtime, not HIR.

use crate::BackendError;
use psrs_core::Module;
use psrs_hir::ExternalKind;
use psrs_runtime::StorageOperation;

pub(super) mod call;
pub(super) mod metadata;
pub(super) mod target;

/// Validate all declarations, including unused ones, before lowering/erasure.
/// The immutable checked Core arena owns every signature identity used here.
///
/// The former State-to-Step source contract check was removed with the State
/// mechanism. Provider identity and export existence are still checked; the
/// operand projection is reported unsupported at the CC boundary instead.
pub(crate) fn validate(module: &Module) -> Result<(), Vec<BackendError>> {
    for external in &module.externals {
        let ExternalKind::Runtime {
            module: provider,
            function,
        } = &external.kind
        else {
            continue;
        };
        let checked = module
            .external_types
            .iter()
            .find(|ty| ty.symbol == external.symbol);
        let source_module = checked.map_or(module.id, |ty| ty.source_module);
        let span = external
            .signature
            .as_ref()
            .map_or(module.span, |ty| ty.span);
        let error = |message| {
            vec![BackendError::new("P8 runtime linking", span, message).with_module(source_module)]
        };
        if provider != psrs_runtime::STORAGE_MODULE {
            return Err(error(format!("unknown runtime provider `{provider}`")));
        }
        StorageOperation::from_export(function).ok_or_else(|| {
            error(format!(
                "runtime provider `{provider}` has no export `{function}`"
            ))
        })?;
    }
    Ok(())
}

//! Checked source contracts for ordinary runtime provider bindings.
//! Export lookup and physical operations belong to psrs-runtime, not HIR.

use crate::BackendError;
use psrs_core::Module;
use psrs_hir::ExternalKind;
use psrs_runtime::StorageOperation;

pub(super) mod call;
pub(super) mod metadata;
mod source;
pub(super) mod target;

/// Validate all declarations, including unused ones, before lowering/erasure.
/// The immutable checked Core arena owns every signature identity used here.
///
/// Source operands are ordinary values; no sequencing protocol is inferred.
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
        let operation = StorageOperation::from_export(function).ok_or_else(|| {
            error(format!(
                "runtime provider `{provider}` has no export `{function}`"
            ))
        })?;
        let checked = checked
            .ok_or_else(|| error("runtime binding has no checked source signature".into()))?;
        source::validate(module, checked.ty, operation).map_err(error)?;
    }
    Ok(())
}

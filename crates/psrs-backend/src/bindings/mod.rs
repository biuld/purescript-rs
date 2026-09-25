//! Backend-side metadata that accompanies target-neutral CC.

use crate::BackendError;
use crate::abi::SourceSignature;
use crate::cc;
use psrs_core::Module as CoreModule;
use psrs_hir::{ExternalKind, SymbolId};
use std::collections::{HashMap, HashSet};

/// The complete input consumed by P9. Platform binding metadata is kept beside
/// CC rather than embedded in the CC module itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendInput {
    pub cc: cc::Module,
    pub externals: ExternalBindings,
    pub warnings: Vec<crate::BackendWarning>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ExternalBindings {
    pub imports: Vec<ExternalBinding>,
}

impl ExternalBindings {
    /// Extracts platform binding metadata while crossing the Core boundary.
    /// CC receives only this side table and therefore never needs to inspect
    /// WIT names or HIR external kinds.
    pub(crate) fn from_core(module: &CoreModule) -> Self {
        let imports = module
            .externals
            .iter()
            .filter_map(|external| {
                let ExternalKind::Wit {
                    interface,
                    function,
                } = &external.kind
                else {
                    return None;
                };
                Some(ExternalBinding {
                    symbol: external.symbol,
                    interface: interface.clone(),
                    function: function.clone(),
                    signature: external
                        .signature
                        .as_ref()
                        .and_then(|signature| crate::abi::source_signature(module, signature)),
                })
            })
            .collect();
        Self { imports }
    }

    /// Checks that the side table is a complete, lossless projection of the
    /// source WIT externals. The check happens before CC lowering so a caller
    /// cannot accidentally make a target binding disappear by supplying a
    /// partial table.
    pub(crate) fn validate_core(&self, module: &CoreModule) -> Result<(), Vec<BackendError>> {
        let expected = module
            .externals
            .iter()
            .filter(|external| matches!(external.kind, ExternalKind::Wit { .. }))
            .map(|external| (external.symbol, external))
            .collect::<HashMap<_, _>>();
        let mut seen = HashSet::new();
        let mut errors = Vec::new();
        for binding in &self.imports {
            if !seen.insert(binding.symbol) {
                errors.push(BackendError::new(
                    "P8 external binding validation",
                    module.span,
                    format!("external binding {:?} is duplicated", binding.symbol),
                ));
                continue;
            }
            let Some(external) = expected.get(&binding.symbol) else {
                errors.push(BackendError::new(
                    "P8 external binding validation",
                    module.span,
                    format!(
                        "external binding {:?} is not a source WIT import",
                        binding.symbol
                    ),
                ));
                continue;
            };
            let expected_signature = external
                .signature
                .as_ref()
                .and_then(|signature| crate::abi::source_signature(module, signature));
            if !same_source_signature(binding.signature.as_ref(), expected_signature.as_ref()) {
                errors.push(BackendError::new(
                    "P8 external binding validation",
                    binding
                        .signature
                        .as_ref()
                        .map_or(module.span, |signature| signature.span),
                    format!(
                        "external binding {:?} has a mismatched source signature",
                        binding.symbol
                    ),
                ));
            }
        }
        for external in expected.values() {
            if !seen.contains(&external.symbol) {
                errors.push(BackendError::new(
                    "P8 external binding validation",
                    module.span,
                    format!(
                        "source WIT import {:?} has no external binding",
                        external.symbol
                    ),
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Checks that P9 receives the same abstract signature that CC used when
    /// type-checking calls. WIT names remain in this table, while CC sees only
    /// the resulting target-neutral signature.
    pub(crate) fn validate_cc(&self, module: &cc::Module) -> Result<(), Vec<BackendError>> {
        let mut cc_externals = HashMap::new();
        let mut errors = Vec::new();
        for external in &module.externals {
            if cc_externals.insert(external.symbol, external).is_some() {
                errors.push(BackendError::new(
                    "P9 external binding validation",
                    module.span,
                    format!("CC external symbol {:?} is duplicated", external.symbol),
                ));
            }
        }
        let mut seen = HashSet::new();
        for binding in &self.imports {
            if !seen.insert(binding.symbol) {
                errors.push(BackendError::new(
                    "P9 external binding validation",
                    module.span,
                    format!("external binding {:?} is duplicated", binding.symbol),
                ));
                continue;
            }
            let Some(external) = cc_externals.get(&binding.symbol) else {
                errors.push(BackendError::new(
                    "P9 external binding validation",
                    module.span,
                    format!("external binding {:?} is absent from CC", binding.symbol),
                ));
                continue;
            };
            let signature_matches = binding
                .signature
                .as_ref()
                .zip(external.signature.as_ref())
                .is_some_and(|(source, actual)| {
                    cc::signature_matches_source(source, actual, &module.representations)
                })
                || binding.signature.is_none() && external.signature.is_none();
            if !signature_matches {
                errors.push(BackendError::new(
                    "P9 external binding validation",
                    binding
                        .signature
                        .as_ref()
                        .map_or(module.span, |signature| signature.span),
                    format!(
                        "external binding {:?} disagrees with its CC signature",
                        binding.symbol
                    ),
                ));
            }
        }
        for external in cc_externals.values() {
            if !seen.contains(&external.symbol) {
                errors.push(BackendError::new(
                    "P9 external binding validation",
                    module.span,
                    format!("CC external {:?} has no binding", external.symbol),
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn same_source_signature(left: Option<&SourceSignature>, right: Option<&SourceSignature>) -> bool {
    left.zip(right).is_some_and(|(left, right)| {
        left.parameters == right.parameters && left.result == right.result
    }) || left.is_none() && right.is_none()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalBinding {
    pub symbol: SymbolId,
    pub interface: String,
    pub function: String,
    pub signature: Option<SourceSignature>,
}

#[cfg(test)]
mod tests;

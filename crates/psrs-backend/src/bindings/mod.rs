//! Backend-side metadata that accompanies target-neutral CC.

use crate::BackendError;
use crate::cc;
use psrs_core::{Module as CoreModule, TypeId as CoreTypeId};
use psrs_hir::{ExternalKind, ModuleId, SymbolId};
use std::collections::{HashMap, HashSet};

mod primitives;
pub(crate) use primitives::lower as lower_primitives;

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
    /// WIT names or HIR external kinds. Each declaration's checked,
    /// synonym-expanded source scheme is read from Core's external signature
    /// table and recorded as a `type_id`, so CC derives its layout from that
    /// identity instead of re-deriving it from raw HIR.
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
                let checked = module
                    .external_types
                    .iter()
                    .find(|checked| checked.symbol == external.symbol);
                Some(ExternalBinding {
                    symbol: external.symbol,
                    source_module: checked.map_or(module.id, |checked| checked.source_module),
                    interface: interface.clone(),
                    function: function.clone(),
                    type_id: checked.map(|checked| checked.ty),
                    span: external
                        .signature
                        .as_ref()
                        .map(|signature| signature.span)
                        .unwrap_or(module.span),
                })
            })
            .collect();
        Self { imports }
    }

    /// Resolves and validates every source WIT binding against the vendored WIT
    /// using the declaration's resolved Core type. This runs where Core is
    /// available, so no source-type mirror is needed and a declaration fails
    /// even when dead code never calls it.
    pub(crate) fn validate_conformance(
        &self,
        module: &CoreModule,
        target: crate::TargetCapabilities,
    ) -> Result<(), Vec<BackendError>> {
        let mut registry = crate::abi::WasiRegistry::load_with_capabilities(target)
            .map_err(|message| vec![BackendError::new("P8 WIT linking", module.span, message)])?;
        for binding in &self.imports {
            let qualified = format!("{}#{}", binding.interface, binding.function);
            let import = registry
                .import(&binding.interface, &binding.function)
                .map_err(|message| {
                    vec![
                        BackendError::new("P8 WIT linking", binding.span, message)
                            .with_module(binding.source_module),
                    ]
                })?;
            if let Some(reason) = &import.unsupported {
                return Err(vec![
                    BackendError::new(
                        "P8 WIT linking",
                        binding.span,
                        format!("WIT import `{qualified}` is unsupported: {reason}"),
                    )
                    .with_module(binding.source_module),
                ]);
            }
            let Some(type_id) = binding.type_id else {
                return Err(vec![
                    BackendError::new(
                        "P8 WIT linking",
                        binding.span,
                        format!("WIT import `{qualified}` has no resolved source type"),
                    )
                    .with_module(binding.source_module),
                ]);
            };
            crate::abi::link::validate_import_signature(&import, module, type_id).map_err(
                |message| {
                    vec![
                        BackendError::new("P8 WIT linking", binding.span, message)
                            .with_module(binding.source_module),
                    ]
                },
            )?;
        }
        Ok(())
    }

    /// Checks that the side table is a complete, lossless projection of the
    /// source WIT externals. The check happens before CC lowering so a caller
    /// cannot accidentally make a target binding disappear by supplying a
    /// partial table.
    pub(crate) fn validate_core(&self, module: &CoreModule) -> Result<(), Vec<BackendError>> {
        let unsupported = module
            .externals
            .iter()
            .filter_map(|external| {
                let ExternalKind::Library { module: owner } = &external.kind else {
                    return None;
                };
                let mut error = BackendError::new(
                    "P8 library linking",
                    external
                        .signature
                        .as_ref()
                        .map_or(module.span, |ty| ty.span),
                    format!(
                        "foreign value `{owner}.{}` has no target implementation",
                        external.name
                    ),
                );
                if let Some(checked) = module
                    .external_types
                    .iter()
                    .find(|checked| checked.symbol == external.symbol)
                {
                    error = error.with_module(checked.source_module);
                }
                Some(error)
            })
            .collect::<Vec<_>>();
        if !unsupported.is_empty() {
            return Err(unsupported);
        }
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
            let Some(_external) = expected.get(&binding.symbol) else {
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
            if !cc_externals.contains_key(&binding.symbol) {
                errors.push(BackendError::new(
                    "P9 external binding validation",
                    module.span,
                    format!("external binding {:?} is absent from CC", binding.symbol),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalBinding {
    pub symbol: SymbolId,
    pub source_module: ModuleId,
    pub interface: String,
    pub function: String,
    /// The declaration's resolved source type, interned in the module type
    /// table. It is the identity CC uses to select the canonical layout.
    pub type_id: Option<CoreTypeId>,
    /// The span of the foreign import's type annotation.
    pub span: psrs_span::TextRange,
}

#[cfg(test)]
mod tests;

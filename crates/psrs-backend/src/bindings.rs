//! Backend-side metadata that accompanies target-neutral CC.

use crate::abi::SourceSignature;
use crate::cc;
use psrs_core::Module as CoreModule;
use psrs_hir::{ExternalKind, SymbolId};

/// The complete input consumed by P9. Platform binding metadata is kept beside
/// CC rather than embedded in the CC module itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendInput {
    pub cc: cc::Module,
    pub externals: ExternalBindings,
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
                        .and_then(crate::abi::source_signature),
                })
            })
            .collect();
        Self { imports }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalBinding {
    pub symbol: SymbolId,
    pub interface: String,
    pub function: String,
    pub signature: Option<SourceSignature>,
}

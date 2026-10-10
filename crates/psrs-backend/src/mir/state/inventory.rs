//! Source requirements remain independent of mutable per-function witnesses.
use super::*;
use psrs_hir::SymbolId;
use std::collections::HashSet;

/// No source inventory is used only for directly constructed physical MIR.
/// Production P9 modules retain their checked logical source even when pure.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Inventory {
    source: Option<Arc<cc::Module>>,
    required: HashSet<SymbolId>,
    declared: HashSet<SymbolId>,
}

impl Inventory {
    pub(in crate::mir) fn checked(source: Arc<cc::Module>) -> Result<Self, Vec<BackendError>> {
        let flows = cc::state::derive_all(&source)?;
        if !flows.is_empty() {
            // Dependency projection requires ordinary CC typing as well.
            // An empty inventory does not publish additional typing claims.
            cc::state::check(&source)?;
        }
        let required = flows.iter().map(|flow| flow.function.symbol).collect();
        let declared = source
            .functions
            .iter()
            .map(|function| function.symbol)
            .collect();
        Ok(Self {
            source: Some(source),
            required,
            declared,
        })
    }

    /// P9 registers only the helpers it generated during this lowering.
    pub(in crate::mir) fn with_helpers(
        mut self,
        helpers: &[Function],
    ) -> Result<Self, Vec<BackendError>> {
        for helper in helpers {
            if helper.state.is_some() || !self.declared.insert(helper.symbol) {
                return Err(error(
                    helper,
                    "generated MIR helper conflicts with source provenance",
                ));
            }
        }
        Ok(self)
    }

    pub(in crate::mir) fn requires(&self, symbol: SymbolId) -> bool {
        self.required.contains(&symbol)
    }

    pub(in crate::mir) fn verify(
        &self,
        module: &crate::mir::Module,
    ) -> Result<(), Vec<BackendError>> {
        let mut errors = Vec::new();
        if self.source.is_none() && module.layout.is_some() {
            errors.push(BackendError::invalid_ir(
                "P9 MIR dependency verification",
                module.span,
                "planned MIR has no checked dependency inventory",
            ));
        }
        // Reachability may remove whole unused functions. Every retained
        // function is still checked against the original source requirement;
        // pruning never rewrites this immutable inventory.
        for function in &module.functions {
            if self.source.is_some() && !self.declared.contains(&function.symbol) {
                errors.extend(error(
                    function,
                    "MIR function has no source or generated-helper provenance",
                ));
            }
            match (self.requires(function.symbol), &function.state) {
                (true, None) => errors.extend(error(
                    function,
                    "MIR function is missing required dependency evidence",
                )),
                (false, Some(_)) => errors.extend(error(
                    function,
                    "MIR dependency evidence has no module source requirement",
                )),
                (true, Some(flow)) if self.source.as_deref() != Some(flow.source.as_ref()) => {
                    errors.extend(error(
                        function,
                        "MIR dependency evidence belongs to a different source inventory",
                    ));
                }
                _ => {}
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
